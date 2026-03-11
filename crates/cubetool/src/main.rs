mod inspect;
mod vfs;

use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use disks::binrw::BinWrite;
use disks::binrw::io::BufReader;
use disks::{dol, iso};
use eyre_pretty::{Context, ContextCompat, Result, bail, eyre};

#[derive(Debug, Subcommand)]
enum Command {
    /// Disassemble a PowerPC instruction.
    Disassemble { code: String },
    /// Inspect a file
    ///
    /// Supported formats: .dol, .iso
    Inspect {
        /// Path to the input file
        #[arg(short, long)]
        input: PathBuf,
        /// Whether to inspect the filesystem (only valid for .iso files)
        #[arg(long, default_value_t = false)]
        filesystem: bool,
    },
    /// Convert a file to another format
    ///
    /// Supported conversions: .dol to .elf
    Convert {
        /// Path to the input file
        #[arg(short, long)]
        input: PathBuf,
        /// Path to the output file
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Extract a file from another
    ///
    /// Supported input formats: .iso
    Extract {
        /// Target to extract
        #[arg(short, long)]
        target: String,
        /// Path to the input file
        #[arg(short, long)]
        input: PathBuf,
        /// Path to the output file
        #[arg(short, long)]
        output: PathBuf,
    },
}

/// A CLI to inspect and manipulate files related to the GameCube.
///
/// Supported formats: .dol, .iso, .elf.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Action to take
    #[command(subcommand)]
    command: Command,
}

fn convert_elf_to_dol(input: PathBuf, output: PathBuf) -> Result<()> {
    let input = std::fs::File::open(&input).context("opening input file")?;
    let dol = dol::elf_to_dol(BufReader::new(input))?;

    let mut output = BufWriter::new(std::fs::File::create(&output).context("opening output file")?);
    dol.write(&mut output)?;

    Ok(())
}

fn extract_bootfile(input: PathBuf, output: PathBuf) -> Result<()> {
    let input = std::fs::File::open(&input).context("opening input file")?;
    let mut iso = iso::Iso::new(BufReader::new(input))?;

    let mut output = BufWriter::new(std::fs::File::create(&output).context("opening output file")?);
    let dol = iso.bootfile()?;
    dol.write(&mut output)?;

    Ok(())
}

fn extract_iso_file(input: PathBuf, output: PathBuf, target: String) -> Result<()> {
    let input = std::fs::File::open(&input).context("opening input file")?;
    let mut iso = iso::Iso::new(BufReader::new(input))?;
    let filesystem = vfs::VirtualFileSystem::new(&mut iso)?;

    let target = filesystem
        .path_to_entry(target)
        .ok_or(eyre!("no entry with such path in the filesystem"))?;

    let entry = filesystem.graph().node_weight(target).unwrap();
    let vfs::VirtualEntry::File(file) = entry else {
        bail!("entry at given path is a directory");
    };

    let mut output = BufWriter::new(std::fs::File::create(&output).context("opening output file")?);
    iso.reader()
        .seek(SeekFrom::Start(file.data_offset as u64))?;

    let mut reader = iso.reader().take(file.data_length as u64);
    std::io::copy(&mut reader, &mut output)?;

    Ok(())
}

fn main() -> Result<()> {
    eyre_pretty::install().unwrap();

    let config = Args::parse();
    match config.command {
        Command::Disassemble { code } => {
            let code = code.replace("_", "");
            let code = if let Some(code) = code.strip_prefix("0x") {
                u32::from_str_radix(code, 16).context("parsing instruction code")?
            } else if let Some(code) = code.strip_prefix("0b") {
                u32::from_str_radix(code, 2).context("parsing instruction code")?
            } else {
                code.parse::<u32>().context("parsing instruction code")?
            };

            let ins = powerpc::Ins::new(code, powerpc::Extensions::gekko_broadway());
            let mut parsed = powerpc::ParsedIns::new();
            ins.parse_basic(&mut parsed);
            println!("{parsed}");

            Ok(())
        }
        Command::Inspect { input, filesystem } => {
            let extension = input
                .extension()
                .and_then(|ext| ext.to_str())
                .context("unknown or missing file extension")?;

            match extension {
                "dol" => inspect::inspect_dol(input),
                "iso" => inspect::inspect_iso(input, filesystem),
                "ciso" | "cso" => inspect::inspect_cso(input),
                "rvz" => inspect::inspect_rvz(input),
                _ => bail!("unknown or missing file extension"),
            }
        }
        Command::Convert { input, output } => {
            let extension = input
                .extension()
                .and_then(|ext| ext.to_str())
                .context("unknown or missing file extension")?;

            match extension {
                "elf" => convert_elf_to_dol(input, output),
                _ => bail!("unknown or missing file extension"),
            }
        }
        Command::Extract {
            target,
            input,
            output,
        } => {
            let extension = input
                .extension()
                .and_then(|ext| ext.to_str())
                .context("unknown or missing file extension")?;

            match (extension, &*target) {
                ("iso", "bootfile") => extract_bootfile(input, output),
                ("iso", _) => extract_iso_file(input, output, target),
                _ => bail!("unsupported extension/target combination"),
            }
        }
    }
}
