pub mod constants {
    //! Shared constants for tests

    use std::time::Duration;

    /// [`PROTOCOL_MAX_BLOCK_SIZE`] controls the maximum block size
    /// of this protocol instance. This is an arbitrary default value.
    pub const PROTOCOL_MAX_BLOCK_SIZE: u64 = 1_000_000;

    /// [`MAX_BLOCK_SIZE_INCREMENT_PERIOD`] controls period between
    /// optimistic increments of maximum block size limit. This is an arbitrary default value.
    pub const MAX_BLOCK_SIZE_INCREMENT_PERIOD: Duration = Duration::from_secs(60);

    /// [`NUM_NODES_IN_VID_COMPUTATION`] controls the number of nodes that are
    /// used in the VID computation for the test.
    pub const NUM_NODES_IN_VID_COMPUTATION: usize = 4;

    /// [`NUM_CONSENSUS_RETRIES`] controls the number of attempts that the
    /// simulated consensus will perform when an error is returned from the
    /// Builder when asking for available blocks.
    pub const NUM_CONSENSUS_RETRIES: usize = 4;

    /// [`CHANNEL_BUFFER_SIZE`] governs the buffer size used for the test
    /// channels. All of the channels created need a capacity.  The specific
    /// capacity isn't specifically bounded, so it is set to an arbitrary value.
    pub const CHANNEL_BUFFER_SIZE: usize = 32;
}
