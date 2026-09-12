//! seccomp-BPF syscall filtering for the Linux sandbox launcher.
//!
//! Translates the policy's [`SeccompProfile`](super::policy::SeccompProfile)
//! into BPF programs installed on the launcher process before it execs the
//! wrapped command. `PR_SET_NO_NEW_PRIVS` is set first so an unprivileged
//! process may install filters; filters survive `execve`.
//!
//! Enforcement:
//! - `blocked_syscalls` → `EPERM` outright.
//! - `clone`/`clone3` → namespace creation blocked unless `allow_namespaces`.
//!   `clone3` passes its flags behind a pointer that BPF cannot dereference,
//!   so it is blocked outright with `ENOSYS` — the errno glibc needs to fall
//!   back to plain `clone`, whose flags *are* filterable.
//! - `socket` → `AF_INET`/`AF_INET6` denied unless `allow_network_sockets`.
//!   Unix sockets stay available (Seatbelt parity: `(allow network* (local
//!   unix))`); the managed network proxy (future work) closes that gap.
//!
//! Domain allowlists are not enforceable here (BPF cannot inspect
//! `connect()` destinations); they fail closed upstream, same as Seatbelt.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule, TargetArch,
};

use super::policy::SeccompProfile;

// Syscall numbers absent from some supported architectures: `umount` was
// replaced by `umount2`, and port-I/O (`iopl`, `ioperm`) is x86-only.
#[cfg(target_arch = "x86_64")]
const SYS_UMOUNT: Option<i64> = Some(libc::SYS_umount);
#[cfg(not(target_arch = "x86_64"))]
const SYS_UMOUNT: Option<i64> = None;
#[cfg(target_arch = "x86_64")]
const SYS_IOPL: Option<i64> = Some(libc::SYS_iopl);
#[cfg(not(target_arch = "x86_64"))]
const SYS_IOPL: Option<i64> = None;
#[cfg(target_arch = "x86_64")]
const SYS_IOPERM: Option<i64> = Some(libc::SYS_ioperm);
#[cfg(not(target_arch = "x86_64"))]
const SYS_IOPERM: Option<i64> = None;

/// Namespace-creation clone flags blocked when `allow_namespaces` is false.
const NAMESPACE_CLONE_FLAGS: &[libc::c_int] = &[
    libc::CLONE_NEWNS,
    libc::CLONE_NEWCGROUP,
    libc::CLONE_NEWUTS,
    libc::CLONE_NEWIPC,
    libc::CLONE_NEWUSER,
    libc::CLONE_NEWPID,
    libc::CLONE_NEWNET,
];

/// Install the seccomp filters described by `profile` on this process.
pub fn apply_seccomp_filter(profile: &SeccompProfile) -> Result<()> {
    if profile.log_only() {
        tracing::debug!("seccomp profile is log_only; installing no filter");
        return Ok(());
    }
    set_no_new_privs()?;

    let arch = target_arch().ok_or_else(|| anyhow!("seccomp filtering is unsupported on this architecture"))?;

    // Primary filter: blocklist + argument-filtered rules → EPERM.
    let primary = SeccompFilter::new(
        primary_rules(profile)?,
        SeccompAction::Allow,
        SeccompAction::Errno(u32::try_from(libc::EPERM).context("EPERM conversion")?),
        arch,
    )
    .map_err(|error| anyhow!("seccomp filter construction failed: {error}"))?;
    install(primary)?;

    // Separate filter so clone3 can return ENOSYS (glibc's fallback trigger)
    // while everything else returns EPERM.
    if !profile.allow_namespaces() {
        let mut clone3 = BTreeMap::new();
        clone3.insert(libc::SYS_clone3, vec![SeccompRule::new(vec![])?]);
        let filter = SeccompFilter::new(
            clone3,
            SeccompAction::Allow,
            SeccompAction::Errno(u32::try_from(libc::ENOSYS).context("ENOSYS conversion")?),
            arch,
        )
        .map_err(|error| anyhow!("seccomp clone3 filter construction failed: {error}"))?;
        install(filter)?;
    }
    Ok(())
}

fn install(filter: SeccompFilter) -> Result<()> {
    let program = BpfProgram::try_from(filter).map_err(|error| anyhow!("seccomp BPF compilation failed: {error}"))?;
    seccompiler::apply_filter(&program).map_err(|error| anyhow!("failed to install seccomp filter: {error}"))
}

/// Build the primary rule set: outright blocks, plus argument-filtered rules
/// for `clone` (namespace flags) and `socket` (INET domains).
fn primary_rules(profile: &SeccompProfile) -> Result<BTreeMap<i64, Vec<SeccompRule>>> {
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();

    for name in profile.blocked_syscalls() {
        match syscall_number(name) {
            Some(nr) => {
                rules.entry(nr).or_default().push(SeccompRule::new(vec![])?);
            }
            None => tracing::debug!(syscall = %name, "syscall absent on this architecture; not blocking"),
        }
    }

    if !profile.allow_namespaces() {
        // clone's flags argument is arg 0 on x86_64, aarch64, and riscv64.
        // MaskedEq(mask) with value == mask matches when all mask bits are
        // set, i.e. "this namespace flag is present"; rules are OR-bound.
        for flag in NAMESPACE_CLONE_FLAGS {
            let mask =
                u64::from(u32::try_from(*flag).map_err(|error| anyhow!("CLONE flag conversion failed: {error}"))?);
            rules
                .entry(libc::SYS_clone)
                .or_default()
                .push(SeccompRule::new(vec![SeccompCondition::new(
                    0,
                    SeccompCmpArgLen::Dword,
                    SeccompCmpOp::MaskedEq(mask),
                    mask,
                )?])?);
        }
    }

    if !profile.allow_network_sockets() {
        for domain in [libc::AF_INET, libc::AF_INET6] {
            rules
                .entry(libc::SYS_socket)
                .or_default()
                .push(SeccompRule::new(vec![SeccompCondition::new(
                    0,
                    SeccompCmpArgLen::Dword,
                    SeccompCmpOp::Eq,
                    u64::try_from(domain).map_err(|error| anyhow!("socket domain conversion failed: {error}"))?,
                )?])?);
        }
    }

    Ok(rules)
}

fn set_no_new_privs() -> Result<()> {
    nix::sys::prctl::set_no_new_privs().map_err(|error| anyhow!("PR_SET_NO_NEW_PRIVS failed: {error}"))
}

fn target_arch() -> Option<TargetArch> {
    #[cfg(target_arch = "x86_64")]
    {
        Some(TargetArch::x86_64)
    }
    #[cfg(target_arch = "aarch64")]
    {
        Some(TargetArch::aarch64)
    }
    #[cfg(target_arch = "riscv64")]
    {
        Some(TargetArch::riscv64)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
    {
        None
    }
}

/// Map a syscall name to its number, or `None` when absent on this
/// architecture (e.g. `iopl`/`ioperm` do not exist on aarch64).
fn syscall_number(name: &str) -> Option<i64> {
    match name {
        "ptrace" => Some(libc::SYS_ptrace),
        "mount" => Some(libc::SYS_mount),
        "umount" => SYS_UMOUNT,
        "umount2" => Some(libc::SYS_umount2),
        "init_module" => Some(libc::SYS_init_module),
        "finit_module" => Some(libc::SYS_finit_module),
        "delete_module" => Some(libc::SYS_delete_module),
        "kexec_load" => Some(libc::SYS_kexec_load),
        "kexec_file_load" => Some(libc::SYS_kexec_file_load),
        "bpf" => Some(libc::SYS_bpf),
        "perf_event_open" => Some(libc::SYS_perf_event_open),
        "userfaultfd" => Some(libc::SYS_userfaultfd),
        "process_vm_readv" => Some(libc::SYS_process_vm_readv),
        "process_vm_writev" => Some(libc::SYS_process_vm_writev),
        "reboot" => Some(libc::SYS_reboot),
        "swapon" => Some(libc::SYS_swapon),
        "swapoff" => Some(libc::SYS_swapoff),
        "settimeofday" => Some(libc::SYS_settimeofday),
        "clock_settime" => Some(libc::SYS_clock_settime),
        "adjtimex" => Some(libc::SYS_adjtimex),
        "add_key" => Some(libc::SYS_add_key),
        "request_key" => Some(libc::SYS_request_key),
        "keyctl" => Some(libc::SYS_keyctl),
        "ioperm" => SYS_IOPERM,
        "iopl" => SYS_IOPL,
        "acct" => Some(libc::SYS_acct),
        "quotactl" => Some(libc::SYS_quotactl),
        "unshare" => Some(libc::SYS_unshare),
        "setns" => Some(libc::SYS_setns),
        "personality" => Some(libc::SYS_personality),
        "clone" => Some(libc::SYS_clone),
        "clone3" => Some(libc::SYS_clone3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_blocked_syscall_maps_to_a_number() {
        for name in super::super::policy::BLOCKED_SYSCALLS {
            // iopl/ioperm legitimately do not exist on non-x86_64.
            let x86_only = matches!(*name, "iopl" | "ioperm");
            if x86_only && !cfg!(target_arch = "x86_64") {
                continue;
            }
            assert!(syscall_number(name).is_some(), "BLOCKED_SYSCALLS entry {name:?} has no number mapping");
        }
    }

    #[test]
    fn primary_rules_block_everything_requested() {
        let profile = SeccompProfile::strict();
        let rules = primary_rules(&profile).unwrap();
        for name in super::super::policy::BLOCKED_SYSCALLS {
            let Some(nr) = syscall_number(name) else { continue };
            assert!(rules.contains_key(&nr), "syscall {name} missing from primary rules");
        }
        assert!(rules.contains_key(&libc::SYS_clone), "namespace-flag clone rules required");
        assert!(rules.contains_key(&libc::SYS_socket), "socket domain rules required");
        // clone3 lives in the separate ENOSYS filter.
        assert!(!rules.contains_key(&libc::SYS_clone3));
    }

    #[test]
    fn permissive_profile_keeps_network_sockets() {
        let profile = SeccompProfile::permissive();
        let rules = primary_rules(&profile).unwrap();
        assert!(!rules.contains_key(&libc::SYS_socket), "allow_network_sockets must not block socket()");
        // clone namespace filtering is still applied (allow_namespaces=false).
        assert!(rules.contains_key(&libc::SYS_clone));
    }
}
