#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use derive_more::derive::Debug;
use hotshot_types::traits::node_implementation::Versions;
use vbs::version::{StaticVersion, Version};

pub mod block;
pub mod coordinator;
pub mod error;
pub mod state;
#[cfg_attr(coverage_nightly, coverage(off))]
pub mod testing;
pub mod utils;

// TODO (jparr721) - Remove this immediately
/// Versions
pub const VERSION_MAJ: u16 = 0;

/// CONSTANT for protocol minor version
pub const VERSION_MIN: u16 = 1;

pub const VERSION_0_1: Version = Version {
    major: VERSION_MAJ,
    minor: VERSION_MIN,
};

/// Constant for the version of this API.
pub const BASE_VERSION: Version = VERSION_0_1;

pub type StaticVersion01 = StaticVersion<VERSION_MAJ, VERSION_MIN>;

/// Specific type for version 0.1
#[derive(Debug, Clone, Copy)]
pub struct Version01;

impl Versions for Version01 {
    type Base = StaticVersion01;

    type Upgrade = StaticVersion01;

    const UPGRADE_HASH: [u8; 32] = [0; 32];

    type Marketplace = StaticVersion01;

    type Epochs = StaticVersion01;
}
