# fs-place (working name)

Is this part of a path the system's? Owned by the system owner, not writable by anybody else, no link on the way.

- `Trust::system().system_owned(part, Level)` - one part of a way. Unix: owner root, no symlink, no write for group or others.
  Windows: path of a local fixed disk only (network, `subst` and removable drives are refused), no reparse point, owner SYSTEM / Administrators / TrustedInstaller, no allow entry of anybody else
  that writes (above the anchor only what can swap the child). Deny entries are not counted: too strict at worst, never too lax.
- `Trust::for_test(owners)` - for tests that need a folder of their own; only with the feature `test-support`.
- `windows::sid`, `windows::acl` - the Windows identity (SID, token owner, peer checks) and rights (`Descriptor`, `make_private`) basics, moved here from `fs-rpc` so there is one copy.

Windows limits, said openly: elevated = Administrators = trusted (like root/sudo on Unix; UAC is no boundary; Backup Operators with SeRestore/SeBackup pass any ACL, like CAP_DAC_OVERRIDE); the check looks at the path, so the caller must check every part from the top.

Security module: a doubt is a refusal. Checked on Windows only by the runner (`windows.yml`). Written with Claude (Anthropic).
