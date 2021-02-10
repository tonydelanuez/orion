// MIT License

// Copyright (c) 2020-2021 The orion Developers

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! # Parameters:
//! - `data`: The data to be hashed.
//!
//! # Errors:
//! An error will be returned if:
//! - [`finalize()`] is called twice without a [`reset()`] in between.
//! - [`update()`] is called after [`finalize()`] without a [`reset()`] in
//!   between.
//!
//! # Panics:
//! A panic will occur if:
//! - More than 2*(2^64-1) __bits__ of data are hashed.
//!
//! # Security:
//! - SHA384 is vulnerable to length extension attacks.
//!
//! # Recommendation:
//! - It is recommended to use [BLAKE2b] when possible.
//!
//! # Example:
//! ```rust
//! use orion::hazardous::hash::sha2::sha384::Sha384;
//!
//! // Using the streaming interface
//! let mut state = Sha384::new();
//! state.update(b"Hello world")?;
//! let hash = state.finalize()?;
//!
//! // Using the one-shot function
//! let hash_one_shot = Sha384::digest(b"Hello world")?;
//!
//! assert_eq!(hash, hash_one_shot);
//! # Ok::<(), orion::errors::UnknownCryptoError>(())
//! ```
//! [`update()`]: struct.Sha384.html
//! [`reset()`]: struct.Sha384.html
//! [`finalize()`]: struct.Sha384.html
//! [BLAKE2b]: ../blake2b/index.html

use super::{ch, maj};
use crate::{
    errors::UnknownCryptoError,
    hazardous::hash::sha2::sha512::{big_sigma_0, big_sigma_1, small_sigma_0, small_sigma_1},
    util::endianness::{load_u64_into_be, store_u64_into_be},
};

/// The blocksize for the hash function SHA384.
pub const SHA384_BLOCKSIZE: usize = 128;
/// The output size for the hash function SHA384.
pub const SHA384_OUTSIZE: usize = 48;

construct_public! {
    /// A type to represent the `Digest` that SHA384 returns.
    ///
    /// # Errors:
    /// An error will be returned if:
    /// - `slice` is not 48 bytes.
    (Digest, test_digest, SHA384_OUTSIZE, SHA384_OUTSIZE)
}

impl_from_trait!(Digest, SHA384_OUTSIZE);

#[rustfmt::skip]
#[allow(clippy::unreadable_literal)]
/// The SHA384 constants as defined in FIPS 180-4.
const K: [u64; 80] = crate::hazardous::hash::sha2::sha512::K;

#[rustfmt::skip]
#[allow(clippy::unreadable_literal)]
/// The SHA384 initial hash value H(0) as defined in FIPS 180-4.
const H0: [u64; 8] = [
    0xcbbb9d5dc1059ed8, 0x629a292a367cd507, 0x9159015a3070dd17, 0x152fecd8f70e5939,
    0x67332667ffc00b31, 0x8eb44a8768581511, 0xdb0c2e0d64f98fa7, 0x47b5481dbefa4fa4,
];

#[derive(Clone)]
/// SHA384 streaming state.
pub struct Sha384 {
    working_state: [u64; 8],
    buffer: [u8; SHA384_BLOCKSIZE],
    leftover: usize,
    message_len: [u64; 2],
    is_finalized: bool,
}

impl Drop for Sha384 {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.working_state.zeroize();
        self.buffer.zeroize();
        self.message_len.zeroize();
    }
}

impl core::fmt::Debug for Sha384 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Sha384 {{ working_state: [***OMITTED***], buffer: [***OMITTED***], leftover: {:?}, \
             message_len: {:?}, is_finalized: {:?} }}",
            self.leftover, self.message_len, self.is_finalized
        )
    }
}

impl Default for Sha384 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha384 {
    func_compress_and_process!(SHA384_BLOCKSIZE, u64, 0u64, load_u64_into_be, 80);

    /// Increment the message length during processing of data.
    fn increment_mlen(&mut self, length: u64) {
        // The checked shift checks that the right-hand side is a legal shift.
        // The result can still overflow if length > u64::MAX / 8.
        // Should be impossible for a user to trigger, because update() processes
        // in SHA384_BLOCKSIZE chunks.
        debug_assert!(length <= u64::MAX / 8);

        // left-shift to get bit-sized representation of length
        // using .unwrap() because it should not panic in practice
        let len = length.checked_shl(3).unwrap();
        let (res, was_overflow) = self.message_len[1].overflowing_add(len);
        self.message_len[1] = res;

        if was_overflow {
            // If this panics size limit is reached.
            self.message_len[0] = self.message_len[0].checked_add(1).unwrap();
        }
    }

    /// Initialize a `Sha384` struct.
    pub fn new() -> Self {
        Self {
            working_state: H0,
            buffer: [0u8; SHA384_BLOCKSIZE],
            leftover: 0,
            message_len: [0u64; 2],
            is_finalized: false,
        }
    }

    /// Reset to `new()` state.
    pub fn reset(&mut self) {
        self.working_state = H0;
        self.buffer = [0u8; SHA384_BLOCKSIZE];
        self.leftover = 0;
        self.message_len = [0u64; 2];
        self.is_finalized = false;
    }

    func_update!(SHA384_BLOCKSIZE, u64);

    /// Return a SHA384 digest.
    fn _finalize_internal(&mut self, digest_dst: &mut [u8]) -> Result<(), UnknownCryptoError> {
        if self.is_finalized {
            return Err(UnknownCryptoError);
        }

        self.is_finalized = true;

        // self.leftover should not be greater than SHA384_BLOCKSIZE
        // as that would have been processed in the update call
        debug_assert!(self.leftover < SHA384_BLOCKSIZE);
        self.buffer[self.leftover] = 0x80;
        self.leftover += 1;

        for itm in self.buffer.iter_mut().skip(self.leftover) {
            *itm = 0;
        }

        // Check for available space for length padding
        if (SHA384_BLOCKSIZE - self.leftover) < 16 {
            self.process(None);
            for itm in self.buffer.iter_mut().take(self.leftover) {
                *itm = 0;
            }
        }

        self.buffer[SHA384_BLOCKSIZE - 16..SHA384_BLOCKSIZE - 8]
            .copy_from_slice(&self.message_len[0].to_be_bytes());
        self.buffer[SHA384_BLOCKSIZE - 8..SHA384_BLOCKSIZE]
            .copy_from_slice(&self.message_len[1].to_be_bytes());

        self.process(None);

        debug_assert!(digest_dst.len() == SHA384_OUTSIZE);
        store_u64_into_be(&self.working_state[..6], digest_dst);

        Ok(())
    }

    #[must_use = "SECURITY WARNING: Ignoring a Result can have real security implications."]
    /// Return a SHA384 digest.
    pub fn finalize(&mut self) -> Result<Digest, UnknownCryptoError> {
        let mut digest = [0u8; SHA384_OUTSIZE];
        self._finalize_internal(&mut digest)?;

        Ok(Digest::from(digest))
    }

    #[must_use = "SECURITY WARNING: Ignoring a Result can have real security implications."]
    /// Calculate a SHA384 digest of some `data`.
    pub fn digest(data: &[u8]) -> Result<Digest, UnknownCryptoError> {
        let mut state = Self::new();
        state.update(data)?;
        state.finalize()
    }
}

impl crate::hazardous::hash::ShaHash for Sha384 {
    fn new() -> Self {
        Sha384::new()
    }

    fn update(&mut self, data: &[u8]) -> Result<(), UnknownCryptoError> {
        self.update(data)
    }

    fn finalize(&mut self, dest: &mut [u8]) -> Result<(), UnknownCryptoError> {
        self._finalize_internal(dest)
    }

    fn digest(data: &[u8], dest: &mut [u8]) -> Result<(), UnknownCryptoError> {
        let mut ctx = Sha384::new();
        ctx.update(data)?;
        ctx._finalize_internal(dest)
    }
}

#[cfg(test)]
/// Compare two Sha384 state objects to check if their fields
/// are the same.
pub fn compare_sha384_states(state_1: &Sha384, state_2: &Sha384) {
    assert_eq!(state_1.working_state, state_2.working_state);
    assert_eq!(state_1.buffer[..], state_2.buffer[..]);
    assert_eq!(state_1.leftover, state_2.leftover);
    assert_eq!(state_1.message_len, state_2.message_len);
    assert_eq!(state_1.is_finalized, state_2.is_finalized);
}

// Testing public functions in the module.
#[cfg(test)]
mod public {
    use super::*;

    #[test]
    fn test_default_equals_new() {
        let new = Sha384::new();
        let default = Sha384::default();
        compare_sha384_states(&new, &default);
    }

    #[test]
    #[cfg(feature = "safe_api")]
    fn test_debug_impl() {
        let initial_state = Sha384::new();
        let debug = format!("{:?}", initial_state);
        let expected = "Sha384 { working_state: [***OMITTED***], buffer: [***OMITTED***], leftover: 0, message_len: [0, 0], is_finalized: false }";
        assert_eq!(debug, expected);
    }

    mod test_streaming_interface {
        use super::*;
        use crate::test_framework::incremental_interface::*;

        impl TestableStreamingContext<Digest> for Sha384 {
            fn reset(&mut self) -> Result<(), UnknownCryptoError> {
                Ok(self.reset())
            }

            fn update(&mut self, input: &[u8]) -> Result<(), UnknownCryptoError> {
                self.update(input)
            }

            fn finalize(&mut self) -> Result<Digest, UnknownCryptoError> {
                self.finalize()
            }

            fn one_shot(input: &[u8]) -> Result<Digest, UnknownCryptoError> {
                Sha384::digest(input)
            }

            fn verify_result(expected: &Digest, input: &[u8]) -> Result<(), UnknownCryptoError> {
                let actual: Digest = Self::one_shot(input)?;

                if &actual == expected {
                    Ok(())
                } else {
                    Err(UnknownCryptoError)
                }
            }

            fn compare_states(state_1: &Sha384, state_2: &Sha384) {
                compare_sha384_states(state_1, state_2)
            }
        }

        #[test]
        fn default_consistency_tests() {
            let initial_state: Sha384 = Sha384::new();

            let test_runner = StreamingContextConsistencyTester::<Digest, Sha384>::new(
                initial_state,
                SHA384_BLOCKSIZE,
            );
            test_runner.run_all_tests();
        }

        // Proptests. Only executed when NOT testing no_std.
        #[cfg(feature = "safe_api")]
        mod proptest {
            use super::*;

            quickcheck! {
                /// Related bug: https://github.com/brycx/orion/issues/46
                /// Test different streaming state usage patterns.
                fn prop_input_to_consistency(data: Vec<u8>) -> bool {
                    let initial_state: Sha384 = Sha384::new();

                    let test_runner = StreamingContextConsistencyTester::<Digest, Sha384>::new(
                        initial_state,
                        SHA384_BLOCKSIZE,
                    );
                    test_runner.run_all_tests_property(&data);
                    true
                }
            }
        }
    }
}

// Testing private functions in the module.
#[cfg(test)]
mod private {
    use super::*;

    mod test_increment_mlen {
        use super::*;

        #[test]
        fn test_mlen_increase_values() {
            let mut context = Sha384 {
                working_state: H0,
                buffer: [0u8; SHA384_BLOCKSIZE],
                leftover: 0,
                message_len: [0u64; 2],
                is_finalized: false,
            };

            context.increment_mlen(1);
            assert!(context.message_len == [0u64, 8u64]);
            context.increment_mlen(17);
            assert!(context.message_len == [0u64, 144u64]);
            context.increment_mlen(12);
            assert!(context.message_len == [0u64, 240u64]);
            // Overflow
            context.increment_mlen(u64::MAX / 8);
            assert!(context.message_len == [1u64, 232u64]);
        }

        #[test]
        #[should_panic]
        fn test_panic_on_second_overflow() {
            let mut context = Sha384 {
                working_state: H0,
                buffer: [0u8; SHA384_BLOCKSIZE],
                leftover: 0,
                message_len: [u64::MAX, u64::MAX - 7],
                is_finalized: false,
            };
            // u64::MAX - 7, to leave so that the length represented
            // in bites should overflow by exactly one.
            context.increment_mlen(1);
        }
    }
}
