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
installer_spec = importlib.util.spec_from_file_location("aps_installer", ROOT / "scripts/install-aps-dkms.py")
installer = importlib.util.module_from_spec(installer_spec)
installer_spec.loader.exec_module(installer)
RELEASE = "7.1.5-76070105-generic"
RUST = "rustc 1.91.1 (ed61e7d7e 2025-11-07) (built from a source tarball)"


class DkmsPolicy(unittest.TestCase):
    def test_sysfs_initializers_follow_target_bindings_not_kernel_version(self):
        old = 'pub struct device_attribute { pub attr: attribute, pub show: Show, pub store: Store, }'
        self.assertEqual(helper.attribute_flags(old), [])
        base = 'pub struct device_attribute { pub attr: attribute, pub __bindgen_anon_1: A, pub __bindgen_anon_2: B, }'
        for kind, flags in [('union', ['--cfg', 'aps_attr_alternatives']),
                            ('struct', ['--cfg', 'aps_attr_alternatives', '--cfg', 'aps_attr_cfi'])]:
            text = (base + f'pub {kind} device_attribute__bindgen_ty_1 {{ pub show: Show, pub show_const: ConstShow, }}'
                    + f'pub {kind} device_attribute__bindgen_ty_2 {{ pub store: Store, pub store_const: ConstStore, }}')
            self.assertEqual(helper.attribute_flags(text), flags)
        for bad in ['', base, old.replace('show:', 'new_show:'), old.replace('pub attr:', 'pub extra: usize, pub attr:')]:
            with self.assertRaises(RuntimeError):
                helper.attribute_flags(bad)

    def test_snapshot_directories_do_not_inherit_a_group_writable_umask(self):
        with patch.object(installer.os, "geteuid", return_value=0), \
             patch.object(installer.os, "umask") as umask, \
             patch.object(installer.dkms, "kernel_name", side_effect=RuntimeError("stop before filesystem access")):
            with self.assertRaisesRegex(RuntimeError, "stop before filesystem"):
                installer.install()
            umask.assert_called_once_with(0o022)

    def test_installed_bytes_must_match_build_not_just_srcversion(self):
        installed = f"/lib/modules/{RELEASE}/updates/dkms/acer_phn16s.ko.zst"
        built = f"/var/lib/dkms/acer-phn16s/{installer.dkms.VERSION}/{RELEASE}/x86_64/module/acer_phn16s.ko.zst"
        contents = {installed: b"old signed module", built: b"new signed module"}
        with patch.object(installer.dkms, "run", return_value=installed), \
             patch.object(installer.dkms, "trusted", side_effect=Path), \
             patch.object(Path, "read_bytes", autospec=True, side_effect=lambda path: contents[str(path)]):
            with self.assertRaisesRegex(RuntimeError, "bytes do not match"):
                installer.verify_installed(RELEASE)
            contents[installed] = contents[built]
            installer.verify_installed(RELEASE)

    def test_installed_module_must_resolve_to_this_kernel_dkms_path(self):
        with patch.object(installer.dkms, "run", return_value="/tmp/acer_phn16s.ko.zst") as run:
            with self.assertRaisesRegex(RuntimeError, "Module resolution"):
                installer.verify_installed(RELEASE)
            run.assert_called_once_with(["modinfo", "-k", RELEASE, "-n", "acer_phn16s"])

    def test_installer_validates_explicit_target_before_touching_system_files(self):
        with patch.object(installer.os, "geteuid", return_value=0), \
             patch.object(installer.os, "umask"), \
             patch.object(installer.os, "uname", side_effect=AssertionError("must use explicit target")):
            with self.assertRaisesRegex(RuntimeError, "Invalid kernel release"):
                installer.install("../bad-target")

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
        args = helper.clang_args(Path("/headers/next-kernel"), {})
        self.assertIn("-I/headers/next-kernel/include", args)
        self.assertFalse(any("7.1.5" in arg for arg in args))

    def test_bindgen_matches_kbuild_fentry_flags_without_disabling_checks(self):
        for values, enabled in [({}, False), ({"CONFIG_HAVE_FENTRY": "n"}, False),
                                ({"CONFIG_HAVE_FENTRY": "y", "CONFIG_FUNCTION_TRACER": "y"}, True)]:
            args = helper.clang_args(Path("/headers"), values)
            self.assertEqual("-mfentry" in args, enabled)
            self.assertEqual("-DCC_USING_FENTRY" in args, enabled)
            self.assertIn("/headers/include/linux/kconfig.h", args)
            self.assertFalse(any(arg.startswith("-U") for arg in args))

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
