//! The helper component behind node's `crypto.pbkdf2`, `scrypt` and `hkdf` (T149): RustCrypto
//! primitives behind the WIT interface `js2wasi3:crypto/primitives`. `crypto.js` checks the
//! arguments and raises node's errors; this only computes.

wit_bindgen::generate!({ path: "wit", world: "helper" });

use exports::js2wasi3::crypto::primitives::Guest;

struct Helper;

/// Fill `out` from `$f::<$h>` for the digest named `$name`.
macro_rules! by_digest {
    ($name:expr, $f:ident, $g:ident, $($arg:expr),*) => {
        match $name {
            "md5" => $f::<md5::Md5>($($arg),*),
            "sha1" => $f::<sha1::Sha1>($($arg),*),
            "sha224" => $f::<sha2::Sha224>($($arg),*),
            "sha256" => $f::<sha2::Sha256>($($arg),*),
            "sha384" => $f::<sha2::Sha384>($($arg),*),
            "sha512" => $f::<sha2::Sha512>($($arg),*),
            "sha512-224" => $f::<sha2::Sha512_224>($($arg),*),
            "sha512-256" => $f::<sha2::Sha512_256>($($arg),*),
            "ripemd160" => $f::<ripemd::Ripemd160>($($arg),*),
            "sm3" => $f::<sm3::Sm3>($($arg),*),
            "sha3-224" => $g::<sha3::Sha3_224>($($arg),*),
            "sha3-256" => $g::<sha3::Sha3_256>($($arg),*),
            "sha3-384" => $g::<sha3::Sha3_384>($($arg),*),
            "sha3-512" => $g::<sha3::Sha3_512>($($arg),*),
            "blake2b512" => $g::<blake2::Blake2b512>($($arg),*),
            "blake2s256" => $g::<blake2::Blake2s256>($($arg),*),
            "md5-sha1" => $g::<Md5Sha1>($($arg),*),
            other => Err(format!("unsupported digest: {other}")),
        }
    };
}

/// OpenSSL's `md5-sha1`: MD5 and SHA-1 of the same data, side by side (36 bytes).
#[derive(Clone, Default)]
struct Md5Sha1 {
    md5: md5::Md5,
    sha1: sha1::Sha1,
}

impl sha3::digest::HashMarker for Md5Sha1 {}

impl sha3::digest::common::BlockSizeUser for Md5Sha1 {
    type BlockSize = sha3::digest::consts::U64;
}

impl sha3::digest::common::OutputSizeUser for Md5Sha1 {
    type OutputSize = sha3::digest::consts::U36;
}

impl sha3::digest::Update for Md5Sha1 {
    fn update(&mut self, data: &[u8]) {
        sha3::digest::Update::update(&mut self.md5, data);
        sha3::digest::Update::update(&mut self.sha1, data);
    }
}

impl sha3::digest::FixedOutput for Md5Sha1 {
    fn finalize_into(self, out: &mut sha3::digest::Output<Self>) {
        out[..16].copy_from_slice(&sha3::Digest::finalize(self.md5));
        out[16..].copy_from_slice(&sha3::Digest::finalize(self.sha1));
    }
}

/// HMAC over any `Digest` (the `hmac` crate wants a block-level core: sha3 and blake2 have none).
struct Mac<D> {
    inner: D,
    outer: D,
}

impl<D: sha3::Digest + sha3::digest::common::BlockSizeUser + Clone> Mac<D> {
    fn new(key: &[u8]) -> Self {
        let mut k = vec![0u8; D::block_size()];
        if key.len() > k.len() {
            let d = D::digest(key);
            k[..d.len()].copy_from_slice(&d);
        } else {
            k[..key.len()].copy_from_slice(key);
        }
        let pad = |x: u8| k.iter().map(|b| b ^ x).collect::<Vec<u8>>();
        Mac { inner: D::new().chain_update(pad(0x36)), outer: D::new().chain_update(pad(0x5c)) }
    }

    fn mac(&self, parts: &[&[u8]]) -> Vec<u8> {
        let mut i = self.inner.clone();
        for p in parts {
            i.update(p);
        }
        self.outer.clone().chain_update(i.finalize()).finalize().to_vec()
    }
}

fn pbkdf2_g<D: sha3::Digest + sha3::digest::common::BlockSizeUser + Clone>(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    out: &mut [u8],
) -> Result<(), String> {
    let m = Mac::<D>::new(password);
    for (i, chunk) in out.chunks_mut(<D as sha3::Digest>::output_size()).enumerate() {
        let mut u = m.mac(&[salt, &(i as u32 + 1).to_be_bytes()]);
        let mut t = u.clone();
        for _ in 1..iterations {
            u = m.mac(&[&u]);
            t.iter_mut().zip(&u).for_each(|(a, b)| *a ^= b);
        }
        chunk.copy_from_slice(&t[..chunk.len()]);
    }
    Ok(())
}

fn hkdf_g<D: sha3::Digest + sha3::digest::common::BlockSizeUser + Clone>(
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
    out: &mut [u8],
) -> Result<(), String> {
    if out.len() > 255 * <D as sha3::Digest>::output_size() {
        return Err("invalid length".into());
    }
    let zeros = vec![0u8; <D as sha3::Digest>::output_size()];
    let prk = Mac::<D>::new(&Mac::<D>::new(if salt.is_empty() { &zeros } else { salt }).mac(&[ikm]));
    let mut t: Vec<u8> = Vec::new();
    for (i, chunk) in out.chunks_mut(<D as sha3::Digest>::output_size()).enumerate() {
        t = prk.mac(&[&t, info, &[i as u8 + 1]]);
        chunk.copy_from_slice(&t[..chunk.len()]);
    }
    Ok(())
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

/// `name`'s digest of `data` (`out_len` bytes for the shake functions).
fn digest(name: &str, data: &[u8], out_len: usize) -> Result<Vec<u8>, String> {
    use sha3::digest::{Digest, ExtendableOutput, XofReader};
    fn plain<D: Digest>(data: &[u8]) -> Vec<u8> {
        D::digest(data).to_vec()
    }
    fn xof<D: ExtendableOutput + Default>(data: &[u8], out_len: usize) -> Vec<u8> {
        let mut h = D::default();
        h.update(data);
        let mut out = vec![0u8; out_len];
        h.finalize_xof().read(&mut out);
        out
    }
    Ok(match name {
        "md5" => plain::<md5::Md5>(data),
        "sha1" => plain::<sha1::Sha1>(data),
        "sha224" => plain::<sha2::Sha224>(data),
        "sha256" => plain::<sha2::Sha256>(data),
        "sha384" => plain::<sha2::Sha384>(data),
        "sha512" => plain::<sha2::Sha512>(data),
        "sha512-224" => plain::<sha2::Sha512_224>(data),
        "sha512-256" => plain::<sha2::Sha512_256>(data),
        "sha3-224" => plain::<sha3::Sha3_224>(data),
        "sha3-256" => plain::<sha3::Sha3_256>(data),
        "sha3-384" => plain::<sha3::Sha3_384>(data),
        "sha3-512" => plain::<sha3::Sha3_512>(data),
        "ripemd160" => plain::<ripemd::Ripemd160>(data),
        "blake2b512" => plain::<blake2::Blake2b512>(data),
        "blake2s256" => plain::<blake2::Blake2s256>(data),
        "sm3" => plain::<sm3::Sm3>(data),
        "md5-sha1" => plain::<Md5Sha1>(data),
        "shake128" => xof::<shake::Shake128>(data, out_len),
        "shake256" => xof::<shake::Shake256>(data, out_len),
        other => return Err(format!("unsupported digest: {other}")),
    })
}

impl Guest for Helper {
    fn digest(name: String, data: Vec<u8>, out_len: u32) -> Result<Vec<u8>, String> {
        digest(&name, &data, out_len as usize)
    }

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
            pbkdf2_g,
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
        by_digest!(digest.as_str(), hkdf, hkdf_g, &ikm, &salt, &info, &mut out)?;
        Ok(out)
    }
}

export!(Helper);
