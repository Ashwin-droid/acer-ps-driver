"""Offline DKMS policy tests. Never build, sign, install, or touch hardware."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("aps_dkms", ROOT / "packaging/dkms/aps_dkms.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
RELEASE = "7.1.5-76070105-generic"
RUST = "rustc 1.91.1 (ed61e7d7e 2025-11-07) (built from a source tarball)"


class DkmsPolicy(unittest.TestCase):
    def test_target_kernel_names_cannot_escape_paths_or_commands(self):
        for value in [RELEASE, "7.1.6-76070106-generic", "7.2.0-rc1"]:
            self.assertEqual(helper.kernel_name(value), value)
        for value in ["", "../7.1.5", "/tmp/7.1.5", "7.1.5;id", "7.1.5\n", "7.1.5 x", "$(id)"]:
            with self.assertRaises(RuntimeError):
                helper.kernel_name(value)

    def test_compiler_must_match_complete_version(self):
        values = {"CONFIG_RUSTC_VERSION_TEXT": RUST}
        with patch.object(Path, "exists", return_value=True), patch.object(helper, "trusted", side_effect=Path):
            with patch.object(helper, "run", return_value=RUST):
                self.assertEqual(helper.compiler(values), Path("/usr/bin/rustc-1.91"))
            with patch.object(helper, "run", return_value="rustc 1.91.1 (different build)"):
                with self.assertRaisesRegex(RuntimeError, "No matching system compiler"):
                    helper.compiler(values)
        with self.assertRaises(RuntimeError):
            helper.rust_version({})

    def test_config_is_data_not_shell(self):
        with tempfile.TemporaryDirectory() as temp:
            file = Path(temp) / ".config"
            file.write_text('CONFIG_RUST=y\nCONFIG_RUSTC_VERSION_TEXT="' + RUST +
                            '"\nCONFIG_OTHER="$(not-executed)"\n')
            values = helper.config(file)
            self.assertEqual(values["CONFIG_RUSTC_VERSION_TEXT"], RUST)
            self.assertEqual(values["CONFIG_OTHER"], "$(not-executed)")

    def test_missing_metadata_fetches_exact_header_package_version(self):
        with patch.object(Path, "exists", return_value=False), \
             patch.object(helper, "run", return_value="7.1.5-distro-build"), \
             patch.object(helper, "download_metadata", return_value=Path("/cache/rust")) as fetch:
            self.assertEqual(helper.metadata(RELEASE, Path("/headers")), Path("/cache/rust"))
            fetch.assert_called_once_with(RELEASE, "7.1.5-distro-build")

    def test_existing_metadata_must_match_header_build(self):
        with patch.object(Path, "exists", return_value=True), \
             patch.object(helper, "run", side_effect=["version-a", "version-b"]):
            with self.assertRaisesRegex(RuntimeError, "does not match"):
                helper.metadata(RELEASE, Path("/headers"))

    def test_generated_binding_parse_handles_unformatted_bindgen(self):
        import re
        text = 'pub const PH_HID_SIZE : u32 = 9 ; pub const PH_DRIVER_SIZE: u32 = 10;'
        self.assertEqual(set(re.findall(r"pub\s+const\s+(PH_[A-Z0-9_]+)\s*:", text)),
                         {"PH_HID_SIZE", "PH_DRIVER_SIZE"})
        args = helper.clang_args(Path("/headers/next-kernel"))
        self.assertIn("-I/headers/next-kernel/include", args)
        self.assertFalse(any("7.1.5" in arg for arg in args))

    def test_failed_prerequisites_remove_stale_module_and_layout(self):
        values = {key: "y" for key in ["CONFIG_X86_64", "CONFIG_RUST", "CONFIG_ACPI", "CONFIG_HID", "CONFIG_INPUT", "CONFIG_PM_SLEEP"]}
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "kernel").mkdir()
            for name in ["acer_phn16s.ko", "hid_layout.rs"]:
                (root / "kernel" / name).write_text("stale")
            with patch.object(helper, "__file__", str(root / "packaging/dkms/aps_dkms.py")), \
                 patch.object(helper, "trusted", side_effect=Path), \
                 patch.object(Path, "read_text", return_value=RELEASE), \
                 patch.object(helper, "config", return_value=values), \
                 patch.object(helper, "compiler", side_effect=RuntimeError("missing compiler")):
                with self.assertRaisesRegex(RuntimeError, "missing compiler"):
                    helper.build(RELEASE)
            self.assertFalse((root / "kernel/acer_phn16s.ko").exists())
            self.assertFalse((root / "kernel/hid_layout.rs").exists())

    def test_development_interface_cannot_pass_production_check(self):
        with tempfile.TemporaryDirectory() as temp:
            module = Path(temp) / "module.ko"
            module.write_bytes(b"ELF dev_probe\0")
            with patch.object(helper, "run", side_effect=["acer_phn16s", RELEASE + " SMP", ""]):
                with self.assertRaisesRegex(RuntimeError, "Developer probe"):
                    helper.check_module(module, RELEASE)

    def test_unsigned_or_wrong_kernel_module_cannot_install(self):
        with patch.object(helper, "run", side_effect=["acer_phn16s", "7.2.0 SMP"]):
            with self.assertRaisesRegex(RuntimeError, "wrong kernel"):
                helper.check_module(Path("/fake"), RELEASE, signed=True)
        with patch.object(helper, "run", side_effect=["acer_phn16s", RELEASE + " SMP", "", ""]):
            with self.assertRaisesRegex(RuntimeError, "unsigned"):
                helper.check_module(Path("/fake"), RELEASE, signed=True)

    def test_enrollment_requires_exact_fingerprint(self):
        with patch.object(helper, "run", return_value="SHA1 Fingerprint: " + helper.FINGERPRINT.lower()):
            helper.enrolled()
        with patch.object(helper, "run", return_value="SHA1 Fingerprint: 00:" + helper.FINGERPRINT):
            with self.assertRaisesRegex(RuntimeError, "not enrolled"):
                helper.enrolled()

    def test_user_writable_sources_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            file = Path(temp) / "source"
            file.write_text("untrusted")
            file.chmod(0o666)
            with self.assertRaisesRegex(RuntimeError, "protected"):
                helper.trusted(file)

    def test_package_build_cannot_enable_developer_mode(self):
        config = (ROOT / "packaging/dkms/dkms.conf").read_text()
        self.assertIn('AUTOINSTALL="yes"', config)
        self.assertIn('BUILD_EXCLUSIVE_CONFIG="CONFIG_RUST', config)
        self.assertNotIn("aps_dev", config)
        source = (ROOT / "scripts/install-aps-dkms.py").read_text()
        self.assertNotIn('"kernel/dev_probe.rs"', source)
        self.assertNotIn('"kernel/hid_layout.rs"', source)


if __name__ == "__main__":
    unittest.main()
