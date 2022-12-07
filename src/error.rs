use snafu::Snafu;
use std::{io, path::PathBuf};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
    #[snafu(display("Unable to open input file {}: {}", path.display(), source))]
    ReadInput { source: io::Error, path: PathBuf },

    #[snafu(display("Unable to create output file {}: {}", path.display(), source))]
    WriteOutput { source: io::Error, path: PathBuf },
}
