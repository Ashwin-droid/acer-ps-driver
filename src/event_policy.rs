// SPDX-License-Identifier: GPL-2.0-only
//! Decide the next one-shot deadline; idle has no deadline and no timer.
pub(crate) fn deadline(
    sleeping: bool,
    wake: Option<u64>,
    lid: Option<u64>,
    lid_open: bool,
    logo: Option<u64>,
    logo_known: bool,
) -> Option<u64> {
    if sleeping {
        return None;
    }
    let lid = lid.filter(|_| !lid_open || wake.is_none());
    let logo = logo.filter(|_| logo_known && wake.is_none());
    [wake, lid, logo].into_iter().flatten().min()
}
