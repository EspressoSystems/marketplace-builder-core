use atomic::Atomic;
use coarsetime::{Duration, Instant};
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy, bytemuck::NoUninit)]
#[repr(C)]
struct MutableState {
    pub max_block_size: u64,
    pub last_block_size_increment: u64,
}

/// Adjustable limits for block size ceiled by
/// maximum block size allowed by the protocol
#[derive(Debug)]
pub struct BlockSizeLimits {
    mutable_state: Atomic<MutableState>,
    pub protocol_max_block_size: u64,
    pub increment_period: Duration,
}

impl BlockSizeLimits {
    /// Never go lower than 10 kilobytes
    pub const MAX_BLOCK_SIZE_FLOOR: u64 = 10_000;
    /// When adjusting max block size, it will be decremented or incremented
    /// by current value / `MAX_BLOCK_SIZE_CHANGE_DIVISOR`
    pub const MAX_BLOCK_SIZE_CHANGE_DIVISOR: u64 = 10;

    pub fn new(protocol_max_block_size: u64, increment_period: std::time::Duration) -> Self {
        Self {
            protocol_max_block_size,
            increment_period: increment_period.into(),
            mutable_state: Atomic::new(MutableState {
                max_block_size: protocol_max_block_size,
                last_block_size_increment: Instant::now().as_ticks(),
            }),
        }
    }

    pub fn max_block_size(&self) -> u64 {
        self.mutable_state
            .load(std::sync::atomic::Ordering::Relaxed)
            .max_block_size
    }

    /// If increment period has elapsed or `force` flag is set,
    /// increment [`Self::max_block_size`] by current value * [`Self::MAX_BLOCK_SIZE_CHANGE_DIVISOR`]
    /// with [`Self::protocol_max_block_size`] as a ceiling
    pub fn try_increment_block_size(&self, force: bool) {
        if force
            || Instant::now().as_ticks().saturating_sub(
                self.mutable_state
                    .load(Ordering::Relaxed)
                    .last_block_size_increment,
            ) >= self.increment_period.as_ticks()
        {
            self.mutable_state
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
                    let max_block_size = std::cmp::min(
                        previous.max_block_size
                            + previous
                                .max_block_size
                                .div_ceil(Self::MAX_BLOCK_SIZE_CHANGE_DIVISOR),
                        self.protocol_max_block_size,
                    );
                    let last_block_size_increment = Instant::now().as_ticks();
                    Some(MutableState {
                        max_block_size,
                        last_block_size_increment,
                    })
                })
                .expect("Closure always returns Some");
        }
    }

    /// Decrement [`Self::max_block_size`] by current value * [`Self::MAX_BLOCK_SIZE_CHANGE_DIVISOR`]
    /// with [`Self::MAX_BLOCK_SIZE_FLOOR`] as a floor
    pub fn decrement_block_size(&self) {
        self.mutable_state
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
                let max_block_size = std::cmp::max(
                    previous.max_block_size.saturating_sub(
                        previous
                            .max_block_size
                            .div_ceil(Self::MAX_BLOCK_SIZE_CHANGE_DIVISOR),
                    ),
                    Self::MAX_BLOCK_SIZE_FLOOR,
                );
                Some(MutableState {
                    max_block_size,
                    last_block_size_increment: previous.last_block_size_increment,
                })
            })
            .expect("Closure always returns Some");
    }
}

#[cfg(test)]
mod tests {
    use marketplace_builder_shared::testing::constants::{
        TEST_MAX_BLOCK_SIZE_INCREMENT_PERIOD, TEST_PROTOCOL_MAX_BLOCK_SIZE,
    };

    use super::*;

    #[test]
    fn test_increment_block_size() {
        let mut block_size_limits = BlockSizeLimits::new(
            TEST_PROTOCOL_MAX_BLOCK_SIZE,
            std::time::Duration::from_millis(25),
        );
        // Simulate decreased limits
        block_size_limits.mutable_state = Atomic::new(MutableState {
            max_block_size: TEST_PROTOCOL_MAX_BLOCK_SIZE / 2,
            last_block_size_increment: Instant::now().as_ticks(),
        });

        // Shouldn't increment, increment period hasn't passed yet
        block_size_limits.try_increment_block_size(false);
        assert!(block_size_limits.max_block_size() == TEST_PROTOCOL_MAX_BLOCK_SIZE / 2);

        // Should increment, increment period hasn't passed yet, but force flag is set
        block_size_limits.try_increment_block_size(true);
        assert!(block_size_limits.max_block_size() > TEST_PROTOCOL_MAX_BLOCK_SIZE / 2);
        let new_size = block_size_limits.max_block_size();

        std::thread::sleep(std::time::Duration::from_millis(30));

        // Should increment, increment period has passed
        block_size_limits.try_increment_block_size(false);
        assert!(block_size_limits.max_block_size() > new_size);
    }

    #[test]
    fn test_decrement_block_size() {
        let block_size_limits = BlockSizeLimits::new(
            TEST_PROTOCOL_MAX_BLOCK_SIZE,
            TEST_MAX_BLOCK_SIZE_INCREMENT_PERIOD,
        );
        block_size_limits.decrement_block_size();
        assert!(block_size_limits.max_block_size() < TEST_PROTOCOL_MAX_BLOCK_SIZE);
    }

    #[test]
    fn test_max_block_size_floor() {
        let block_size_limits = BlockSizeLimits::new(
            BlockSizeLimits::MAX_BLOCK_SIZE_FLOOR + 1,
            TEST_MAX_BLOCK_SIZE_INCREMENT_PERIOD,
        );
        block_size_limits.decrement_block_size();
        assert_eq!(
            block_size_limits.max_block_size(),
            BlockSizeLimits::MAX_BLOCK_SIZE_FLOOR
        );
    }
}
