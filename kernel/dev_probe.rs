//! Only compiled with APS_DEV=1. Signed raw data, never executable probe code.
use kernel::prelude::*;
#[allow(non_camel_case_types, non_upper_case_globals, dead_code, unreachable_pub)]
#[path = "dev_crypto.rs"]
mod crypto;
#[path = "../src/dev_protocol.rs"]
#[allow(unreachable_pub)]
pub(crate) mod wire;
const PUBLIC_KEY: &[u8] = include_bytes!("dev_probe_public.der");
pub(crate) struct Developer {
    pub(crate) nonce: [u8;32], pub(crate) sequence:u64,
    pub(crate) last_error:i32, pub(crate) active:bool,
    pub(crate) response:[u8;65], pub(crate) response_len:usize,
    last_attempt:u64,
}
impl Developer {
    pub(crate) fn new() -> Self {
        let mut nonce=[0;32];
        // SAFETY: Owned output buffer of exactly the supplied size.
        unsafe { crypto::get_random_bytes(nonce.as_mut_ptr().cast(),nonce.len()) };
        Self { nonce,sequence:0,last_error:0,active:false,response:[0;65],response_len:0,last_attempt:0 }
    }
    pub(crate) fn authenticate(&mut self,text:&[u8],now:u64)->Result<wire::Probe> {
        let text=text.strip_suffix(b"\n").unwrap_or(text);
        let packet=wire::unhex::<{wire::PACKET_LEN}>(text).ok_or(EINVAL)?;
        let probe=wire::Probe::decode(&packet[..wire::MESSAGE_LEN]).ok_or(EINVAL)?;
        if !probe.fresh(&self.nonce,self.sequence,now) { return Err(Error::from_errno(-(kernel::bindings::ESTALE as i32))); }
        if self.last_attempt!=0 && now.saturating_sub(self.last_attempt)<100_000_000 { return Err(EBUSY); }
        self.last_attempt=now;
        let mut packet=KBox::new(packet,GFP_KERNEL)?;
        let mut digest=KBox::new([0u8;32],GFP_KERNEL)?;
        // SAFETY: Fixed bounded buffers, valid throughout synchronous crypto.
        unsafe { crypto::sha256(packet.as_ptr(),wire::MESSAGE_LEN,digest.as_mut_ptr()) };
        let key=crypto::public_key {
            key:PUBLIC_KEY.as_ptr().cast_mut().cast(),keylen:PUBLIC_KEY.len() as u32,
            algo:crypto::OID_OID_rsaEncryption,params:core::ptr::null_mut(),paramlen:0,
            key_is_private:false,id_type:c"APS developer".as_char_ptr().cast(),
            pkey_algo:c"rsa".as_char_ptr().cast(),key_eflags:0,
        };
        let sig=crypto::public_key_signature {
            auth_ids:[core::ptr::null_mut();3],s:packet[wire::MESSAGE_LEN..].as_mut_ptr(),
            m:digest.as_mut_ptr(),s_size:wire::SIGNATURE_LEN as u32,m_size:32,m_free:false,
            algo_takes_data:false,pkey_algo:c"rsa".as_char_ptr().cast(),
            hash_algo:c"sha256".as_char_ptr().cast(),encoding:c"pkcs1".as_char_ptr().cast(),
        };
        // SAFETY: Types generated from exact local kernel headers. Public DER is
        // immutable; data/signature storage is owned and not freed by this API.
        let rc=unsafe { crypto::public_key_verify_signature(&key,&sig) };
        if rc!=0 { return Err(Error::from_errno(rc)); }
        // Consume before firmware access, even if it fails ambiguously.
        self.sequence=self.sequence.checked_add(1).ok_or(EOVERFLOW)?;
        self.response.fill(0); self.response_len=0;
        Ok(probe)
    }
}
