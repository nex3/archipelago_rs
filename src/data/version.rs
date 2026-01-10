use std::fmt;

use crate::protocol::NetworkVersion;

/// A version of Archipelago, including the server and the generator.
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct Version {
    major: u16,
    minor: u16,
    build: u16,
}

impl Version {
    /// The major version number.
    pub fn major(&self) -> u16 {
        self.major
    }

    /// The minor version number.
    pub fn minor(&self) -> u16 {
        self.minor
    }

    /// The build version number.
    pub fn build(&self) -> u16 {
        self.build
    }
}

impl From<NetworkVersion> for Version {
    fn from(network: NetworkVersion) -> Self {
        Version {
            major: network.major,
            minor: network.minor,
            build: network.build,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.build)
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}
