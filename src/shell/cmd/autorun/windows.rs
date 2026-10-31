//! Current-user registry adapter. No HKLM access, shell execution or reg.exe calls.

use super::{Plan, RawValue, Snapshot};
use anyhow::{bail, Context, Result};
use std::io;
use winreg::enums::*;
use winreg::transaction::Transaction;
use winreg::{RegKey, RegValue};

pub const USER_KEY: &str = r"Software\Microsoft\Command Processor";
pub const AUTORUN_VALUE: &str = "AutoRun";
pub const RECEIPT_VALUE: &str = "Hanamori.AutoRun.v1";

/// A snapshot of the real HKCU AutoRun location; constructing it never creates keys.
pub struct UserPlan {
    plan: Plan,
    key: String,
}

impl UserPlan {
    pub fn removal_for(loader: &std::path::Path) -> Result<Option<Self>> {
        Ok(Store(USER_KEY).removal_for(loader)?.map(|plan| Self {
            plan,
            key: USER_KEY.into(),
        }))
    }

    /// `None` removes only the owned leading invocation. Loader files are managed separately.
    pub fn new(loader: Option<&str>) -> Result<Self> {
        Ok(Self {
            plan: Store(USER_KEY).plan(loader)?,
            key: USER_KEY.to_owned(),
        })
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn apply(&self) -> Result<()> {
        Store(&self.key).apply(&self.plan)
    }

    #[cfg(test)]
    pub(crate) fn for_test_key(key: &str, loader: Option<&str>) -> Result<Self> {
        assert!(key.starts_with(r"Software\Hanamori-AutoRun-Test-"));
        Ok(Self {
            plan: Store(key).plan(loader)?,
            key: key.to_owned(),
        })
    }
}

// The alternate location is private and used by tests to avoid the user's real AutoRun.
struct Store<'a>(&'a str);

fn raw(key: &RegKey, name: &str) -> Result<Option<RawValue>> {
    match key.get_raw_value(name) {
        Ok(value) => Ok(Some(RawValue {
            kind: value.vtype as u32,
            bytes: value.bytes.into_owned(),
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("cannot read CMD registry value {name}")),
    }
}

fn snapshot(key: &RegKey) -> Result<Snapshot> {
    Ok(Snapshot {
        autorun: raw(key, AUTORUN_VALUE)?,
        receipt: raw(key, RECEIPT_VALUE)?,
    })
}

fn write(key: &RegKey, name: &str, value: Option<&RawValue>) -> Result<()> {
    if let Some(value) = value {
        let vtype = match value.kind {
            super::plan::REG_SZ => REG_SZ,
            super::plan::REG_EXPAND_SZ => REG_EXPAND_SZ,
            super::plan::REG_BINARY => REG_BINARY,
            _ => bail!("unsupported planned registry value type"),
        };
        key.set_raw_value(
            name,
            &RegValue {
                vtype,
                bytes: value.bytes.as_slice().into(),
            },
        )
        .with_context(|| format!("cannot stage CMD registry value {name}"))?;
    } else {
        match key.delete_value(name) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot remove CMD registry value {name}"))
            }
        }
    }
    Ok(())
}

impl Store<'_> {
    fn removal_for(&self, loader: &std::path::Path) -> Result<Option<Plan>> {
        Plan::removal_for(self.read()?, loader)
    }

    fn read(&self) -> Result<Snapshot> {
        match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(self.0, KEY_QUERY_VALUE) {
            Ok(key) => snapshot(&key),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Snapshot::default()),
            Err(error) => Err(error).context("cannot read the current-user CMD registry key"),
        }
    }

    fn plan(&self, loader: Option<&str>) -> Result<Plan> {
        Plan::new(self.read()?, loader)
    }

    fn apply(&self, plan: &Plan) -> Result<()> {
        self.apply_before_commit(plan, || Ok(()))
    }

    fn apply_before_commit(
        &self,
        plan: &Plan,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if !plan.changed() {
            // A no-op still detects a stale preview, without needing write permissions.
            return plan.verify_snapshot(&self.read()?);
        }
        let transaction = Transaction::new().context("cannot start a CMD registry transaction; no values were changed (non-transactional fallback is disabled)")?;
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey_transacted_with_flags(
                self.0,
                &transaction,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
            )
            .context(
                "cannot open the current-user CMD key in a transaction; no values were changed",
            )?;
        plan.verify_snapshot(&snapshot(&key)?)?;
        // Conflict detection and the two-value publication belong to the OS transaction,
        // not a process-local mutex or a manager-specific lock file.
        if plan.before().autorun != plan.after().autorun {
            write(&key, AUTORUN_VALUE, plan.after().autorun.as_ref())?;
        }
        if plan.before().receipt != plan.after().receipt {
            write(&key, RECEIPT_VALUE, plan.after().receipt.as_ref())?;
        }
        before_commit()?;
        transaction.commit().context(
            "cannot commit CMD initialization; inspect the current registry values and retry",
        )
    }
}

#[cfg(test)]
mod tests;
