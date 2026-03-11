use std::io::{Read, Seek};
use std::path::PathBuf;

use bytesize::ByteSize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use disks::binrw::BinRead;
use disks::binrw::io::BufReader;
use disks::cso::CsoReader;
use disks::iso::{self, Meta};
use disks::rvz::{self, RvzReader};
use disks::{Console, apploader, cso, dol};
use eyre_pretty::{Context, Result};

use crate::vfs::{self, VfsEntryId, VfsGraph, VirtualEntry};

fn label(cells: impl IntoIterator<Item = String>) {
    let mut label = Table::new();
    label
        .load_preset(comfy_table::presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(cells.into_iter());

    println!("{}", label);
}

fn dol_table(header: &dol::Header) {
    let mut sections = Table::new();
    sections
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Section").set_alignment(CellAlignment::Center),
            Cell::new("Offset").set_alignment(CellAlignment::Center),
            Cell::new("Target").set_alignment(CellAlignment::Center),
            Cell::new("Length").set_alignment(CellAlignment::Center),
            Cell::new("Length (Bytes)").set_alignment(CellAlignment::Center),
        ]);

    let mut row = |name, section: dol::SectionInfo| {
        sections.add_row(vec![
            Cell::new(name),
            Cell::new(format!("0x{:08X}", section.offset)),
            Cell::new(format!("0x{:08X}", section.target)),
            Cell::new(format!("0x{:08X}", section.size)),
            Cell::new(format!("{}", ByteSize(section.size as u64).display()))
                .set_alignment(CellAlignment::Center),
        ]);
    };

    for (i, section) in header.text_sections().enumerate() {
        row(format!(".text{i}"), section)
    }

    for (i, section) in header.data_sections().enumerate() {
        row(format!(".data{i}"), section)
    }

    if header.bss_size != 0 {
        sections.add_row(vec![
            Cell::new(".bss"),
            Cell::new("-").set_alignment(CellAlignment::Center),
            Cell::new(format!("0x{:08X}", header.bss_target)),
            Cell::new(format!("0x{:08X}", header.bss_size)),
            Cell::new(format!("{}", ByteSize(header.bss_size as u64).display()))
                .set_alignment(CellAlignment::Center),
        ]);
    }

    println!("{sections}");
}

pub fn inspect_dol(input: PathBuf) -> Result<()> {
    let mut file = std::fs::File::open(&input).context("opening file")?;
    let header = dol::Header::read(&mut file).context("parsing .dol header")?;
    let meta = file.metadata()?;

    let mut info = Table::new();
    info.load_preset(comfy_table::presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new(format!(
                "{} ({})",
                input.file_name().unwrap().to_string_lossy(),
                ByteSize(meta.len()).display()
            )),
            Cell::new(format!("Entry: 0x{:08X}", header.entry)),
        ]);

    label([
        format!(
            "{} ({})",
            input.file_name().unwrap().to_string_lossy(),
            ByteSize(meta.len()).display()
        ),
        format!("Entry: 0x{:08X}", header.entry),
    ]);
    println!("{info}");
    dol_table(&header);

    Ok(())
}

fn apploader_table(header: &apploader::Header) {
    let mut properties = Table::new();
    properties
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Property").set_alignment(CellAlignment::Center),
            Cell::new("Value").set_alignment(CellAlignment::Center),
        ]);

    properties.add_row(vec![
        Cell::new("Version"),
        Cell::new(format!("{}", header.version)),
    ]);

    properties.add_row(vec![
        Cell::new("Entrypoint"),
        Cell::new(format!("0x{:08X}", header.entrypoint)),
    ]);

    properties.add_row(vec![
        Cell::new("Size"),
        Cell::new(format!(
            "0x{:08X} ({})",
            header.size,
            ByteSize(header.size as u64)
        )),
    ]);

    properties.add_row(vec![
        Cell::new("Trailer Size"),
        Cell::new(format!(
            "0x{:08X} ({})",
            header.size,
            ByteSize(header.size as u64)
        )),
    ]);

    println!("{properties}");
}

fn print_dir(graph: &VfsGraph, id: VfsEntryId, depth: u8, current: &str) {
    let VirtualEntry::Dir(dir) = graph.node_weight(id).unwrap() else {
        unreachable!()
    };

    let base = format!(
        "{current}{}{}",
        if current.is_empty() { "" } else { "/" },
        dir.name
    );

    let indent = |offset| {
        for _ in 0..(depth + offset) {
            print!(" ")
        }
    };

    indent(0);
    println!(
        "{}/",
        if dir.name.is_empty() && current.is_empty() {
            "root"
        } else {
            &dir.name
        }
    );

    for child in graph.neighbors(id) {
        let entry = graph.node_weight(child).unwrap();
        match entry {
            VirtualEntry::File(file) => {
                indent(2);
                println!(
                    "{} ({}/{}) ({})",
                    file.name,
                    base,
                    file.name,
                    ByteSize(file.data_length as u64)
                );
            }
            VirtualEntry::Dir(_) => {
                print_dir(graph, child, depth + 2, &base);
            }
        }
    }
}

fn inspect_iso_fs(mut iso: iso::Iso<impl Read + Seek>) -> Result<()> {
    let filesystem = vfs::VirtualFileSystem::new(&mut iso)?;
    let root = filesystem.root();
    let graph = filesystem.graph();

    print_dir(graph, root, 0, "");

    Ok(())
}

fn debug_or_unknown(value: Option<impl std::fmt::Debug>) -> String {
    value.map_or("<unknown>".to_owned(), |x| format!("{x:?}"))
}

fn disk_meta_table(meta: &Meta) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Property").set_alignment(CellAlignment::Center),
            Cell::new("Value").set_alignment(CellAlignment::Center),
        ]);

    table.add_row(vec![
        Cell::new("Game Name"),
        Cell::new(format!("{}", meta.game_name)),
    ]);

    table.add_row(vec![
        Cell::new("Game ID"),
        Cell::new(format!("0x{:04X}", meta.game_id)),
    ]);

    table.add_row(vec![
        Cell::new("Console ID"),
        Cell::new(format!(
            "0x{:02X} ({})",
            meta.console_id,
            debug_or_unknown(meta.console())
        )),
    ]);

    table.add_row(vec![
        Cell::new("Country Code"),
        Cell::new(format!(
            "0x{:02X} ({})",
            meta.country_code,
            debug_or_unknown(meta.region())
        )),
    ]);

    table.add_row(vec![
        Cell::new("Game Code"),
        Cell::new(format!(
            "{} (0x{:04X})",
            meta.game_code_str().as_deref().unwrap_or("<invalid>"),
            meta.game_code()
        )),
    ]);

    table.add_row(vec![
        Cell::new("Maker Code"),
        Cell::new(format!("0x{:04X}", meta.maker_code)),
    ]);

    table.add_row(vec![
        Cell::new("Disk ID"),
        Cell::new(format!("0x{:02X}", meta.disk_id)),
    ]);

    table.add_row(vec![
        Cell::new("Version"),
        Cell::new(format!("0x{:02X}", meta.version)),
    ]);

    table
}

fn disk_properties_table(header: &iso::Header) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Property").set_alignment(CellAlignment::Center),
            Cell::new("Value").set_alignment(CellAlignment::Center),
        ]);

    table.add_row(vec![
        Cell::new("Bootfile Offset"),
        Cell::new(format!("0x{:08X}", header.bootfile_offset)),
    ]);

    table.add_row(vec![
        Cell::new("Debug Monitor Offset"),
        Cell::new(format!("0x{:08X}", header.debug_monitor_offset)),
    ]);

    table.add_row(vec![
        Cell::new("Debug Monitor Target"),
        Cell::new(format!("0x{:08X}", header.debug_monitor_target)),
    ]);

    table.add_row(vec![
        Cell::new("Filesystem Offset"),
        Cell::new(format!("0x{:08X}", header.filesystem_offset)),
    ]);

    table.add_row(vec![
        Cell::new("Filesystem Size"),
        Cell::new(format!(
            "0x{:08X} ({})",
            header.filesystem_size,
            ByteSize(header.filesystem_size as u64)
        )),
    ]);

    table.add_row(vec![
        Cell::new("Max. Filesystem Size"),
        Cell::new(format!(
            "0x{:08X} ({})",
            header.max_filesystem_size,
            ByteSize(header.max_filesystem_size as u64)
        )),
    ]);

    table.add_row(vec![
        Cell::new("Audio Streaming"),
        Cell::new(format!(
            "0x{:02X} ({})",
            header.meta.audio_streaming,
            debug_or_unknown(header.meta.audio_streaming())
        )),
    ]);

    table.add_row(vec![
        Cell::new("Stream Buffer Size"),
        Cell::new(format!(
            "0x{:02X} ({})",
            header.meta.stream_buffer_size,
            ByteSize(header.meta.stream_buffer_size as u64)
        )),
    ]);

    table.add_row(vec![
        Cell::new("User Position"),
        Cell::new(format!("0x{:08X}", header.user_position)),
    ]);

    table.add_row(vec![
        Cell::new("User Length"),
        Cell::new(format!(
            "0x{:08X} ({})",
            header.user_length,
            ByteSize(header.user_length as u64)
        )),
    ]);

    table
}

pub fn inspect_iso(input: PathBuf, filesystem: bool) -> Result<()> {
    let mut file = std::fs::File::open(&input).context("opening file")?;
    let meta = file.metadata()?;
    let mut iso = iso::Iso::new(BufReader::new(&mut file)).context("parsing .iso header")?;

    label([format!(
        "{} ({})",
        input.file_name().unwrap().to_string_lossy(),
        ByteSize(meta.len()).display()
    )]);

    if filesystem {
        return inspect_iso_fs(iso);
    }

    let header = iso.header();
    let disk_meta = disk_meta_table(&header.meta);
    let disk_properties = disk_properties_table(header);

    label(["> Disk Properties".into()]);
    println!("{disk_properties}");
    label(["> Disk Meta".into()]);
    println!("{disk_meta}");

    if let Ok(apploader) = iso.apploader_header() {
        label(["> Apploader".into()]);
        apploader_table(&apploader);
    }

    if let Ok(bootfile) = iso.bootfile_header() {
        label([
            "> Bootfile (.dol)".to_string(),
            format!("Entry: 0x{:08X}", bootfile.entry),
        ]);
        dol_table(&bootfile);
    }

    Ok(())
}

pub fn inspect_cso(input: PathBuf) -> Result<()> {
    let mut file = std::fs::File::open(&input).context("opening file")?;
    let meta = file.metadata()?;
    let cso = cso::Cso::new(BufReader::new(&mut file)).context("parsing .cso header")?;
    let mut cso = CsoReader::new(cso);

    label([format!(
        "{} ({})",
        input.file_name().unwrap().to_string_lossy(),
        ByteSize(meta.len()).display()
    )]);

    let disk_header = cso.iso_header().context("reading ISO header from CISO")?;
    let ciso_header = cso.inner().header();

    let disk_properties = disk_properties_table(&disk_header);

    let mut ciso_properties = Table::new();
    ciso_properties
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Property").set_alignment(CellAlignment::Center),
            Cell::new("Value").set_alignment(CellAlignment::Center),
        ]);

    ciso_properties.add_row(vec![
        Cell::new("Map Size"),
        Cell::new(format!("{} bytes", ciso_header.map.len()))
    ]);

    ciso_properties.add_row(vec![
        Cell::new("Block Size"),
        Cell::new(format!("{}", ByteSize(ciso_header.block_size as u64))),
    ]);

    label(["> CISO Properties".into()]);
    println!("{ciso_properties}");
    label(["> Disk Properties".into()]);
    println!("{disk_properties}");

    if let Ok(apploader) = cso.apploader_header() {
        label(["> Apploader".into()]);
        apploader_table(&apploader);
    }

    if let Ok(bootfile) = cso.bootfile_header() {
        label([
            "> Bootfile (.dol)".to_string(),
            format!("Entry: 0x{:08X}", bootfile.entry),
        ]);
        dol_table(&bootfile);
    }

    Ok(())
}

pub fn inspect_rvz(input: PathBuf) -> Result<()> {
    let mut file = std::fs::File::open(&input).context("opening file")?;
    let meta = file.metadata()?;

    let rvz = rvz::Rvz::new(BufReader::new(&mut file)).context("parsing .rvz file")?;
    let mut rvz = RvzReader::new(rvz);

    label([format!(
        "{} ({})",
        input.file_name().unwrap().to_string_lossy(),
        ByteSize(meta.len()).display()
    )]);

    let disk_header = rvz.iso_header().unwrap();
    let rvz_header = rvz.inner().rvz_header();
    let rvz_disk_header = rvz.inner().disk_header();

    let disk_properties = disk_properties_table(&disk_header);
    let disk_meta = disk_meta_table(&rvz_disk_header.disk_meta);

    let mut rvz_properties = Table::new();
    rvz_properties
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Property").set_alignment(CellAlignment::Center),
            Cell::new("Value").set_alignment(CellAlignment::Center),
        ]);

    rvz_properties.add_row(vec![
        Cell::new("Version"),
        Cell::new(rvz_header.inner.version.to_string()),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("Compatible Version"),
        Cell::new(rvz_header.inner.compatible_version.to_string()),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("Disk Length"),
        Cell::new(ByteSize(rvz_header.inner.disk_len).to_string()),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("RVZ Length"),
        Cell::new(ByteSize(rvz_header.inner.rvz_len).to_string()),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("Console"),
        Cell::new(debug_or_unknown(rvz_disk_header.console)),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("Compression"),
        Cell::new(format!(
            "{:?} (Level {})",
            rvz_disk_header.compression, rvz_disk_header.compression_level
        )),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("Chunk Length"),
        Cell::new(format!("{}", ByteSize(rvz_disk_header.chunk_len as u64))),
    ]);

    if rvz_disk_header.console == Some(Console::Wii) {
        rvz_properties.add_row(vec![
            Cell::new("Partitions"),
            Cell::new(rvz_disk_header.partitions_count.to_string()),
        ]);

        rvz_properties.add_row(vec![
            Cell::new("Partitions Offset"),
            Cell::new(format!("0x{:08X}", rvz_disk_header.partitions_offset)),
        ]);

        rvz_properties.add_row(vec![
            Cell::new("Partitions Length"),
            Cell::new(format!("0x{:08X}", rvz_disk_header.partitions_len)),
        ]);
    }

    rvz_properties.add_row(vec![
        Cell::new("Disk Sections"),
        Cell::new(rvz_disk_header.disk_sections_count.to_string()),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("Disk Sections Offset"),
        Cell::new(format!("0x{:08X}", rvz_disk_header.disk_sections_offset)),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("Disk Sections Length"),
        Cell::new(format!("0x{:08X}", rvz_disk_header.disk_sections_len)),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("File Sections"),
        Cell::new(rvz_disk_header.file_sections_count.to_string()),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("File Sections Offset"),
        Cell::new(format!("0x{:08X}", rvz_disk_header.file_sections_offset)),
    ]);

    rvz_properties.add_row(vec![
        Cell::new("File Sections Length"),
        Cell::new(format!("0x{:08X}", rvz_disk_header.file_sections_len)),
    ]);

    label(["> RVZ Properties".into()]);
    println!("{rvz_properties}");
    label(["> Disk Properties".into()]);
    println!("{disk_properties}");
    label(["> Disk Meta".into()]);
    println!("{disk_meta}");

    if let Ok(apploader) = rvz.apploader_header() {
        label(["> Apploader".into()]);
        apploader_table(&apploader);
    }

    if let Ok(bootfile) = rvz.bootfile_header() {
        label([
            "> Bootfile (.dol)".to_string(),
            format!("Entry: 0x{:08X}", bootfile.entry),
        ]);
        dol_table(&bootfile);
    }

    Ok(())
}
