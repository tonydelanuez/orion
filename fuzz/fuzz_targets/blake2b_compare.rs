#![no_main]
#[macro_use]
extern crate libfuzzer_sys;
extern crate blake2_rfc;
extern crate orion;
pub mod util;

use self::util::*;
use orion::hazardous::hash::blake2b;

fn fuzz_blake2b_non_keyed(data: &[u8], outsize: usize) {
	let mut context = blake2_rfc::blake2b::Blake2b::new(outsize);
	context.update(data);

	let mut state = blake2b::init(None, outsize).unwrap();
	state.update(data).unwrap();

	if data.len() > 512 {
		context.update(b"");
		state.update(b"").unwrap();
	}
	if data.len() > 1028 {
		context.update(b"Extra");
		state.update(b"Extra").unwrap();
	}
	if data.len() > 2049 {
		context.update(&[0u8; 256]);
		state.update(&[0u8; 256]).unwrap();
	}

	let other_hash = context.finalize();
	let orion_hash = state.finalize().unwrap();

	assert_eq!(other_hash.as_bytes(), orion_hash.as_bytes());
}

fn fuzz_blake2b_keyed(data: &[u8], outsize: usize) {
	let mut key = [0u8; 64];
	apply_from_input_fixed(&mut key, data, 0);
	let orion_key = blake2b::SecretKey::from_slice(&key).unwrap();

	let mut context = blake2_rfc::blake2b::Blake2b::with_key(outsize, &key);
	context.update(data);

	let mut state = blake2b::init(Some(&orion_key), outsize).unwrap();
	state.update(data).unwrap();

	if data.len() > 512 {
		context.update(b"");
		state.update(b"").unwrap();
	}
	if data.len() > 1028 {
		context.update(b"Extra");
		state.update(b"Extra").unwrap();
	}
	if data.len() > 2049 {
		context.update(&[0u8; 256]);
		state.update(&[0u8; 256]).unwrap();
	}

	let other_hash = context.finalize();
	let orion_hash = state.finalize().unwrap();

	assert_eq!(other_hash.as_bytes(), orion_hash.as_bytes());
}

fuzz_target!(|data: &[u8]| {
    for out in 1..65 {
	   fuzz_blake2b_non_keyed(data, out);
	   fuzz_blake2b_keyed(data, out);
    }
});
