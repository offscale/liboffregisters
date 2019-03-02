#[macro_use]
extern crate failure;

#[macro_use]
extern crate lazy_static;

pub trait OffRegisters {
    fn already_setup() -> Result<bool, failure::Error>;
    fn pre_install() -> Result<(), failure::Error>;
    fn install() -> Result<(), failure::Error>;
    fn post_install() -> Result<(), failure::Error>;
    fn uninstall() -> Result<(), failure::Error>;
}

pub mod archive;
pub mod download;
pub mod env;
pub mod fs;
