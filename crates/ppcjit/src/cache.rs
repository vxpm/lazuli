use std::hash::{Hash, Hasher};
use std::path::Path;

use cranelift::codegen::isa::TargetIsa;
use fjall::{Database, KeyspaceCreateOptions};
use zerocopy::IntoBytes;

use crate::{Artifact, CodegenSettings, Sequence};

struct Hash128(twox_hash::XxHash3_128);

impl Hasher for Hash128 {
    fn finish(&self) -> u64 {
        unimplemented!()
    }

    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ArtifactKey(u128);

impl ArtifactKey {
    pub fn new(isa: &dyn TargetIsa, settings: &CodegenSettings, seq: &Sequence) -> Self {
        let mut hasher = Hash128(twox_hash::XxHash3_128::with_seed(0));
        isa.name().hash(&mut hasher);
        isa.triple().hash(&mut hasher);
        isa.flags().hash(&mut hasher);
        isa.isa_flags_hash_key().hash(&mut hasher);
        settings.hash(&mut hasher);
        seq.hash(&mut hasher);
        Self(hasher.0.finish_128())
    }
}

pub struct Cache {
    db: Database,
    pending: u16,
    compressor: zstd::bulk::Compressor<'static>,
    decompressor: zstd::bulk::Decompressor<'static>,
    decompress_buffer: Vec<u8>,
}

impl Cache {
    pub fn new(path: impl AsRef<Path>) -> Self {
        _ = std::fs::create_dir(&path);

        let db = Database::builder(&path)
            .journal_compression(fjall::CompressionType::None)
            .manual_journal_persist(true)
            .open()
            .unwrap();

        Self {
            db,
            pending: 0,
            compressor: zstd::bulk::Compressor::new(5).unwrap(),
            decompressor: zstd::bulk::Decompressor::new().unwrap(),
            decompress_buffer: vec![0; 4 * 1024 * 1024],
        }
    }

    pub fn get(&mut self, key: ArtifactKey) -> Option<Artifact> {
        let artifacts = self
            .db
            .keyspace("artifacts", KeyspaceCreateOptions::default)
            .unwrap();

        let artifact = artifacts.get(key.0.as_bytes()).unwrap()?;

        // decompress
        let count = self
            .decompressor
            .decompress_to_buffer(&artifact, &mut self.decompress_buffer)
            .unwrap();

        // deserialize
        let deserialized = rmp_serde::from_slice(&self.decompress_buffer[..count]).unwrap();
        Some(deserialized)
    }

    pub fn insert(&mut self, key: ArtifactKey, compiled: &Artifact) {
        let artifacts = self
            .db
            .keyspace("artifacts", KeyspaceCreateOptions::default)
            .unwrap();

        // serialize
        let serialized = rmp_serde::to_vec(&compiled).unwrap();

        // compress
        let compressed = self.compressor.compress(&serialized).unwrap();
        artifacts.insert(key.0.as_bytes(), compressed).unwrap();

        self.pending += 1;
        if self.pending >= 256 {
            self.pending = 0;
            self.db.persist(fjall::PersistMode::Buffer).unwrap();
        }
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        self.db.persist(fjall::PersistMode::SyncAll).unwrap();
    }
}
