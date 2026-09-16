# Signed raw firmware probes

```bash
bash scripts/build-kernel.sh          # production; dev API compiled out
APS_DEV=1 bash scripts/build-kernel.sh # development; signed raw calls enabled
```

There is no runtime toggle. Read-only `dev_mode` reports the build choice.
Only dev builds contain `dev_challenge`, `dev_probe`, the developer public key,
or signature-verification code. Read `dev_mode` to check the installed build.
Normal `aps` controls stay unsigned and independently bounded.

## Keys and setup

```bash
cargo build --release
target/release/aps-probe keygen             # once; refuses to overwrite
target/release/aps-probe public-key > kernel/dev_probe_public.der
bash scripts/generate-dev-crypto.sh
APS_DEV=1 bash scripts/build-kernel.sh
```

The separate RSA-2048 developer private key is user-owned, mode 0600, under
`$XDG_CONFIG_HOME/aps/dev-signing/private.pem`, defaulting to
`~/.config/aps/dev-signing/private.pem`. Its directory is 0700. Only the public
key is embedded in the module. Never distribute the private key. Any process
that can read it can authorize raw firmware calls, including software running
as the same user. Installing a production build removes this authority; rotating
it requires a dev rebuild with another public key.

This is separate from Secure Boot. Ashwin Module Signing Key still signs the
module. OpenSSL signs developer requests and the kernel verifies them with its
[public-key API](https://cdn.kernel.org/doc/html/latest/crypto/asymmetric-keys.html).

## Use

`/usr/local/sbin/aps-probe` runs as the normal account, without sudo or polkit.

```bash
aps-probe status
aps-probe selftest                    # no firmware access
aps-probe hid lighting get a100000000 # target-list read
aps-probe wmi 05 01020000 8           # raw CPU RPM getter
aps-probe restore                    # restore ordinary APS preferences; end session
aps-probe finish                     # end WITHOUT undoing raw firmware changes
```

| Transport | Fixed destination | Buffer/framing limits |
| --- | --- | --- |
| `hid lighting get/set HEX` | ENEK5130 0CF2:5130 | A1/get/5, A2/set/2, A3/get/9, A4/set/11 bytes, including ID |
| `hid ec get/set HEX` | EC HID 1025:174B | A0, exactly 65 bytes including ID |
| `wmi HEX_METHOD INPUT_HEX OUTPUT_BYTES` | `\_SB.PC00.WMID.WMBH`, instance 0 | Arbitrary u32 method; 4 or 8 input/output bytes |

Payload contents bypass the normal command/value allow-list. WMI responses
include the firmware status byte. No kernel pointers, executable code, arbitrary
devices or arbitrary ACPI paths are accepted. A signed call can still damage
hardware, alter unrelated settings or hang in firmware. Signing proves authority,
not hardware safety. There is no automatic retry or general rollback. `restore`
restores only ordinary APS preferences, not arbitrary changes or fan overrides.

## Request and persistence checks

The fixed 136-byte `APSDEV01` message includes a random 32-byte module-instance
nonce, sequence, expiry, transport metadata, method, padded 65-byte payload and
zero reserved bytes. A 256-byte RSA-PKCS1-v1.5/SHA-256 signature follows. Sysfs
accepts exactly 784 hex characters, optionally one trailing newline. Ordinary
controls retain the 128-byte input limit in both builds.

Signature, nonce, sequence, expiry and bounds are checked before firmware I/O.
Valid requests consume their sequence before I/O, even on failure. Expiry is at
most 30 seconds ahead, with 10 seconds used by the CLI. Verification attempts are
limited to one per 100 ms. Responses are bounded to 65 bytes; signed calls are
logged in dmesg. Runtime activation and replay state are not read from disk.

Raw calls pause preference capture, including shutdown saves. `restore` reapplies
saved normal settings and resumes capture. `finish` resumes capture without
undoing anything. Experimental requests are not normal preference records.

## Verification

```bash
cargo test
cargo clippy --all-targets -- -D warnings
python3 scripts/verify-dev-api.py
python3 scripts/verify-dev-builds.py PRODUCTION.ko DEVELOPMENT.ko kernel/dev_probe_public.der
```

Live checks cover signature forgery, message tampering, replay, malformed inputs,
all three raw transports using known reads, and preference isolation. Binary
checks prove that production excludes dev endpoints, key, domain and crypto
imports. `BUILD_MODE` in the kernel bundle identifies its variant.
