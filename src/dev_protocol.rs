//! Signed dev firmware envelopes. Raw contents, fixed transports and buffer bounds.
#![allow(dead_code)]
pub const DOMAIN: &[u8; 8] = b"APSDEV01";
pub const MESSAGE_LEN: usize = 136;
pub const SIGNATURE_LEN: usize = 256;
pub const PACKET_LEN: usize = MESSAGE_LEN + SIGNATURE_LEN;
pub const TEXT_LEN: usize = PACKET_LEN * 2;
pub const MAX_TTL_NS: u64 = 30_000_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub nonce: [u8; 32],
    pub sequence: u64,
    pub expires: u64,
    // 0=self-test, 1=lighting HID, 2=EC HID, 3=Gaming WMI, 4=end session.
    pub operation: u8,
    // HID: 0=get, 1=set. WMI: zero, since methods can have arbitrary effects.
    pub action: u8,
    pub length: u8,
    pub output_length: u8,
    pub method: u32,
    pub data: [u8; 65],
}
impl Probe {
    pub fn empty() -> Self {
        Self {
            nonce: [0; 32],
            sequence: 0,
            expires: 0,
            operation: 0,
            action: 0,
            length: 0,
            output_length: 0,
            method: 0,
            data: [0; 65],
        }
    }
    pub fn encode(self) -> [u8; MESSAGE_LEN] {
        let mut b = [0; MESSAGE_LEN];
        b[..8].copy_from_slice(DOMAIN);
        b[8..40].copy_from_slice(&self.nonce);
        b[40..48].copy_from_slice(&self.sequence.to_le_bytes());
        b[48..56].copy_from_slice(&self.expires.to_le_bytes());
        b[56] = self.operation;
        b[57] = self.action;
        b[58] = self.length;
        b[59] = self.output_length;
        b[60..64].copy_from_slice(&self.method.to_le_bytes());
        b[64..129].copy_from_slice(&self.data);
        b
    }
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() != MESSAGE_LEN || &b[..8] != DOMAIN || b[129..].iter().any(|v| *v != 0) {
            return None;
        }
        let p = Self {
            nonce: b[8..40].try_into().ok()?,
            sequence: u64::from_le_bytes(b[40..48].try_into().ok()?),
            expires: u64::from_le_bytes(b[48..56].try_into().ok()?),
            operation: b[56],
            action: b[57],
            length: b[58],
            output_length: b[59],
            method: u32::from_le_bytes(b[60..64].try_into().ok()?),
            data: b[64..129].try_into().ok()?,
        };
        p.valid().then_some(p)
    }
    pub fn valid(&self) -> bool {
        if self.length as usize > self.data.len()
            || self.data[self.length as usize..].iter().any(|v| *v != 0)
        {
            return false;
        }
        match self.operation {
            0 | 4 => {
                self.length == 0 && self.action == 0 && self.output_length == 0 && self.method == 0
            }
            1 => {
                self.method == 0
                    && self.output_length == self.length
                    && matches!(
                        (self.data[0], self.length, self.action),
                        (0xa1, 5, 0) | (0xa2, 2, 1) | (0xa3, 9, 0) | (0xa4, 11, 1)
                    )
            }
            2 => {
                self.method == 0
                    && self.data[0] == 0xa0
                    && self.length == 65
                    && self.output_length == 65
                    && self.action <= 1
            }
            3 => {
                self.action == 0
                    && matches!(self.length, 4 | 8)
                    && matches!(self.output_length, 4 | 8)
            }
            _ => false,
        }
    }
    pub fn fresh(&self, nonce: &[u8; 32], seq: u64, now: u64) -> bool {
        self.nonce == *nonce
            && self.sequence == seq
            && self.expires > now
            && self.expires - now <= MAX_TTL_NS
    }
}
pub fn unhex<const N: usize>(text: &[u8]) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let nibble = |b| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    let mut out = [0; N];
    for (i, pair) in text.chunks_exact(2).enumerate() {
        out[i] = nibble(pair[0])? * 16 + nibble(pair[1])?;
    }
    Some(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_envelope_and_freshness() {
        let p = Probe {
            nonce: [7; 32],
            sequence: 4,
            expires: 10_000,
            ..Probe::empty()
        };
        assert_eq!(Probe::decode(&p.encode()), Some(p));
        assert!(p.fresh(&p.nonce, 4, 9000));
        assert!(!p.fresh(&[8; 32], 4, 9000));
        assert!(!p.fresh(&p.nonce, 5, 9000));
        assert!(!p.fresh(&p.nonce, 4, 10_000));
        assert!(!Probe {
            expires: u64::MAX,
            ..p
        }
        .fresh(&p.nonce, 4, 0));
        for len in 0..MESSAGE_LEN {
            assert!(Probe::decode(&p.encode()[..len]).is_none());
        }
        for index in [0, 129, 135] {
            let mut b = p.encode();
            b[index] ^= 1;
            assert!(Probe::decode(&b).is_none());
        }
        assert!(unhex::<1>(b"0g").is_none());
        assert!(unhex::<1>(b"00\n").is_none());
    }
    #[test]
    fn raw_payloads_do_not_relax_memory_or_transport_bounds() {
        let mut p = Probe {
            operation: 3,
            length: 8,
            output_length: 8,
            method: u32::MAX,
            ..Probe::empty()
        };
        p.data[..8].fill(255);
        assert!(p.valid());
        p.length = 255;
        assert!(!p.valid());
        p.length = 8;
        p.output_length = 255;
        assert!(!p.valid());
        p = Probe {
            operation: 1,
            action: 1,
            length: 11,
            output_length: 11,
            ..Probe::empty()
        };
        p.data[..11].fill(255);
        p.data[0] = 0xa4;
        assert!(p.valid());
        p.data[0] = 0xa0;
        assert!(!p.valid());
        p.operation = 255;
        assert!(!p.valid());
    }
}
