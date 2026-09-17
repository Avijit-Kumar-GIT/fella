# Python sandbox

Fella runs model-generated Python in a small WebAssembly guest. The guest is
compiled from [`python-sandbox/src/lib.rs`](../python-sandbox/src/lib.rs) for
`wasm32-unknown-unknown`, then embedded as
`src-tauri/resources/fella-python-sandbox.wasm`. The desktop host executes it
with Wasmi from `analytics/pyexec.rs`.

The guest is intentionally not a WASI module. It receives no operating-system
imports, so Python cannot open a path, inspect environment variables, create a
process, or make a network request. The host links three functions:

| Host function | Purpose |
| --- | --- |
| `fella.write(ptr, len, stream)` | Capture `print()` and tracebacks into the tool result. |
| `fella.sql(query_ptr, query_len, out_ptr, out_cap)` | Run one checked read-only query in the host data engine and copy a bounded JSON result into guest memory. |
| `fella.random(ptr, len)` | Supply OS entropy needed by RustPython hash maps. It carries no workspace or process capability. |

The guest turns the SQL response into a Python `list[dict]`. It does not ship
pandas, NumPy, SciPy, a package installer, or a Python installation. The host
prepends the small pure-Python analytics helpers `median`, `stdev`, `pearsonr`,
and `linregress` to each snippet.

Wasmi applies a fresh Store per run with a 256 MiB linear-memory limit, a 2 MiB
value-stack limit, 1 billion instructions of fuel, 64 KiB of source and output,
10,000 SQL rows, and a 1 MiB SQL response. SQLite also interrupts the host
query after its normal query timeout. These are bounds on a generated
calculation, not a claim that any runtime vulnerability is impossible.

To rebuild the guest after changing its source:

```sh
./scripts/build-python-sandbox.sh
```

The script runs from the guest package directory so its bare-WASM `getrandom`
configuration is applied. The generated module is checked into the source tree
because desktop builds should not require RustPython or a Python installation
on the user's machine.

The guest artifact and the Wasmi/RustPython versions are part of Fella's
trusted computing base. A release process should eventually verify the
artifact hash during packaging and test the guest with hostile snippets before
calling this a stronger enterprise security boundary.

## Longer-term OS sandbox

The embedded guest is the right default for personal analytics, but it is not
an OS boundary for arbitrary native code. If Fella eventually allows generated
Python to use `subprocess`, native binaries, package installers, or direct file
APIs, the execution worker should also run inside a restricted operating-system
process.

The design reference is Cursor's [implementation of a secure sandbox for local
agents](https://cursor.com/blog/agent-sandboxing). Its most useful idea for
Fella is a **uniform sandbox API with platform-specific implementations**. The
security primitives are different on macOS, Linux, and Windows, so one binary
should share a policy and result contract rather than pretend that one native
mechanism works everywhere. Cursor also applies the sandbox to the whole
subprocess tree, generates policy from the workspace, and teaches the agent
what a denied operation means so it changes course instead of retrying the same
command.

Fella should adapt that pattern to a read-only analytics product:

```text
UI and agent loop
        |
        v
capability broker
  - owns the mounted folder and credentials
  - validates read-only data requests
  - returns bounded rows and evidence
        |
        v
restricted fella-sandbox worker
  - empty environment, no credentials
  - no network by default
  - read-only input and private scratch output
  - Wasmi + RustPython guest
```

The main app should remain the only component that opens the user's folder and
the app-data directory. The worker should receive a dataset handle or a
brokered, read-only SQL request. When a direct filesystem view is needed for a
large input, it should receive an allowlisted read-only snapshot or mount, plus
a separate scratch directory. It should never inherit the user's home
directory, `auth.json`, the application database, or the original folder's
write permission.

### Runner contract

The OS layer should be an execution backend behind one small interface. The
agent, evidence model, and UI should not know which operating-system primitive
was selected.

```rust
pub struct SandboxPolicy {
    pub read_roots: Vec<PathBuf>,
    pub write_root: PathBuf,
    pub network: NetworkPolicy,       // denied by default
    pub processes: ProcessPolicy,     // denied by default
    pub env: EnvPolicy,               // empty by default
    pub limits: ResourceLimits,
}

pub trait SandboxRunner: Send + Sync {
    fn execute(
        &self,
        request: SandboxRequest,
        policy: SandboxPolicy,
    ) -> EngineResult<SandboxResult>;
}
```

The first backend remains `WasmRunner`, which needs no OS-specific dependency.
The future `OsWorkerRunner` launches a fixed Fella helper and applies the
policy before it starts generated code. The helper communicates over a framed
local IPC channel, returns bounded stdout/stderr and structured denials, and is
terminated as a process group when it exceeds its deadline. A fresh Wasmi
store is still required inside the worker; the process boundary is additional
defense in depth, not a reason to remove the existing fuel and memory limits.

The policy should be capability-based and fail closed:

| Capability | Personal default | Future opt-in behavior |
| --- | --- | --- |
| Read mounted data | Allow through the broker or an explicit read-only root | Narrow to selected files or a staged snapshot |
| Write source data | Deny | Keep denied; write only to run scratch |
| Network | Deny | Separate explicit permission with a visible explanation |
| Environment and credentials | Empty | Never pass secrets into generated code |
| Child processes | Deny | Only in a separate native-execution mode |
| CPU, memory, output, rows | Bounded | Per-run policy, with hard upper limits |

### Platform backends

These backends are implementation targets, not three new analytics engines:

- **Linux:** create a restricted worker with a private process/filesystem view,
  Landlock rules for the allowed input and scratch paths, seccomp filtering for
  unsafe system calls, and cgroup or rlimit resource limits. Cursor combines
  Landlock with seccomp and uses an overlay filesystem to make ignored files
  inaccessible. Fella can use a staged read-only dataset instead, which fits
  its read-only workflow and avoids copying a developer workspace model. The
  [Landlock API](https://www.kernel.org/doc/html/latest/userspace-api/landlock.html)
  is unprivileged and can restrict the worker and its future children.
- **macOS:** evaluate a fixed, signed helper with App Sandbox entitlements
  first. Cursor uses a dynamically generated Seatbelt profile through
  `sandbox-exec` because App Sandbox would complicate arbitrary binaries;
  Fella's default worker is fixed and does not need to execute generated
  binaries, so the tradeoff is different. Seatbelt is deprecated and must be
  treated as a compatibility backend with version-specific tests, not an
  assumption that it will remain the long-term API. The [Apple App Sandbox
  documentation](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox)
  describes the kernel-enforced entitlement model.
- **Windows:** investigate a native restricted worker using AppContainer or
  LPAC, a restricted token, Job Object limits, and process-tree termination.
  Cursor currently runs its Linux sandbox inside WSL2 because an equivalent
  general-purpose native Windows sandbox is difficult to compose. Fella should
  not add a WSL2 requirement to the lightweight personal build; if the native
  backend is unavailable, remain on the WASM-only path rather than silently
  running generated native code without a boundary. [Windows AppContainer
  guidance](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)
  documents explicit resource grants and the more restricted LPAC model.

The shared policy must be stricter than the platform-specific implementation.
For example, a platform backend may technically support network access, but
the Fella policy should still report `network denied` unless that capability is
explicitly part of a future product flow.

### Agent and UI behavior

Sandboxing changes the harness as well as the process launcher. Tool results
should distinguish a Python error from a policy denial:

```json
{
  "kind": "sandbox_denied",
  "capability": "network",
  "operation": "socket.connect",
  "message": "This analysis can use mounted data but cannot access the network."
}
```

The system prompt should state the active capabilities in plain language. A
denial should tell the model which constraint stopped it and whether changing
the query can solve the problem. The harness should stop repeated identical
attempts and return a clear answer when the requested analysis needs a denied
capability. Cursor found that surfacing the specific sandbox constraint in
tool results materially improved recovery from failed attempts; Fella should
measure the same behavior with analytics questions rather than adding a broad
approval flow for non-technical users.

### Implementation sequence

1. **Define the policy and result types.** Keep `WasmRunner` as the only active
   backend and record the effective capabilities in Python evidence.
2. **Extract a worker protocol.** Move Wasmi/RustPython behind a small helper
   process, with brokered SQL, framed IPC, process-group cleanup, and the same
   resource caps. This gives the app a second boundary without changing the
   model tools.
3. **Ship Linux first.** Add Landlock, seccomp, private filesystem setup, and
   hard failure when the required restrictions cannot be installed. Test denied
   reads, writes, sockets, environment access, child-process access, and
   process-tree cleanup.
4. **Add macOS and Windows adapters.** Keep the policy contract identical and
   document any capability differences. The app must select WASM-only mode
   when an OS adapter is unavailable; it must not downgrade silently to an
   unrestricted subprocess.
5. **Evaluate native subprocess support separately.** If arbitrary native
   programs become a real product requirement, assess a utility VM or
   hypervisor-isolated runner. That is a stronger boundary but adds startup,
   memory, packaging, and support costs that do not belong in the personal
   default today.

### Security acceptance tests

Before calling the OS backend a stronger security boundary, the test suite
should launch hostile fixtures and prove that the worker cannot:

- read a sentinel file outside its approved roots or discover `auth.json`;
- write the mounted folder or the app database;
- read environment variables or use a socket;
- escape restrictions through a child process or symlink;
- exceed CPU, memory, output, row, or wall-clock limits; or
- leave a running descendant after cancellation or timeout.

The tests need to run on every supported OS and on the oldest supported kernel
or OS release. A missing primitive, an unexpected permission error, or a
policy-construction failure is a sandbox setup error and must fail closed. The
personal version does not need an admin policy service, remote execution,
multi-tenant scheduling, or enterprise control plane; those are separate
deployment concerns.
