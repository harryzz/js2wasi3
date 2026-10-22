//! The helper component behind node's `crypto.pbkdf2`, `scrypt` and `hkdf` (T149): RustCrypto
//! primitives behind the WIT interface `js2wasi3:crypto/primitives`. `crypto.js` checks the
//! arguments and raises node's errors; this only computes.

wit_bindgen::generate!({ path: "wit", world: "helper" });

use exports::js2wasi3::crypto::primitives::Guest;

struct Helper;

/// Fill `out` from `$f::<$h>` for the digest named `$name`.
macro_rules! by_digest {
    ($name:expr, $f:ident, $($arg:expr),*) => {
        match $name {
            "md5" => $f::<md5::Md5>($($arg),*),
            "sha1" => $f::<sha1::Sha1>($($arg),*),
            "sha224" => $f::<sha2::Sha224>($($arg),*),
            "sha256" => $f::<sha2::Sha256>($($arg),*),
            "sha384" => $f::<sha2::Sha384>($($arg),*),
            "sha512" => $f::<sha2::Sha512>($($arg),*),
            other => Err(format!("unsupported digest: {other}")),
        }
    };
}

fn pbkdf2<D>(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) -> Result<(), String>
where
    D: hmac::digest::block_api::EagerHash + Clone,
    hmac::Hmac<D>: hmac::digest::KeyInit + hmac::digest::Update + hmac::digest::FixedOutput + Clone,
{
    pbkdf2::pbkdf2_hmac::<D>(password, salt, iterations, out);
    Ok(())
}

fn hkdf<D>(ikm: &[u8], salt: &[u8], info: &[u8], out: &mut [u8]) -> Result<(), String>
where
    D: hmac::digest::block_api::EagerHash + Clone,
    hmac::Hmac<D>: hmac::digest::KeyInit + hmac::digest::Update + hmac::digest::FixedOutput + Clone,
{
    hkdf::Hkdf::<D>::new(Some(salt), ikm)
        .expand(info, out)
        .map_err(|e| e.to_string())
}

impl Guest for Helper {
    fn pbkdf2(
        password: Vec<u8>,
        salt: Vec<u8>,
        iterations: u32,
        keylen: u32,
        digest: String,
    ) -> Result<Vec<u8>, String> {
        let mut out = vec![0u8; keylen as usize];
        by_digest!(
            digest.as_str(),
            pbkdf2,
            &password,
            &salt,
            iterations,
            &mut out
        )?;
        Ok(out)
    }

    fn scrypt(
        password: Vec<u8>,
        salt: Vec<u8>,
        keylen: u32,
        cost: u32,
        block_size: u32,
        parallelization: u32,
        _maxmem: u64,
    ) -> Result<Vec<u8>, String> {
        if cost < 2 || !cost.is_power_of_two() {
            return Err("N must be a power of two greater than 1".into());
        }
        let params = scrypt::Params::new(cost.trailing_zeros() as u8, block_size, parallelization)
            .map_err(|e| e.to_string())?;
        let mut out = vec![0u8; keylen as usize];
        scrypt::scrypt(&password, &salt, &params, &mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }

    fn hkdf(
        digest: String,
        ikm: Vec<u8>,
        salt: Vec<u8>,
        info: Vec<u8>,
        keylen: u32,
    ) -> Result<Vec<u8>, String> {
        let mut out = vec![0u8; keylen as usize];
        by_digest!(digest.as_str(), hkdf, &ikm, &salt, &info, &mut out)?;
        Ok(out)
    }
}

export!(Helper);
