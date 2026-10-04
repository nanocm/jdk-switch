pub mod adoptium;
pub mod traits;
pub mod downloader;
pub mod extractor;
pub mod progress;
pub mod corretto;
pub mod openjdk;
pub mod zulu;

use crate::error::Result;
use clap::ValueEnum;
use traits::JdkSource;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Vendor {
    #[value(alias = "adoptium")]
    Temurin,
    #[value(alias = "amazon")]
    Corretto,
    #[value(alias = "azul")]
    Zulu,
    Openjdk,
}

impl Vendor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Temurin => "temurin",
            Self::Corretto => "corretto",
            Self::Zulu => "zulu",
            Self::Openjdk => "openjdk",
        }
    }

    pub fn source(self) -> Result<Box<dyn JdkSource>> {
        match self {
            Self::Temurin => Ok(Box::new(adoptium::AdoptiumSource::new()?)),
            Self::Corretto => Ok(Box::new(corretto::CorrettoSource::new()?)),
            Self::Zulu => Ok(Box::new(zulu::ZuluSource::new()?)),
            Self::Openjdk => Ok(Box::new(openjdk::OpenJdkSource::new()?)),
        }
    }
}
