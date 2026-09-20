//! Process runner interface, real XeLaTeX/Biber invocation, and the
//! bounded multipass compile orchestration.

pub mod logs;

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInvocation {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    /// The complete child environment. [`RealProcessRunner`] clears the
    /// inherited environment and applies exactly this list, so callers
    /// build it with `toolchain::prepare_child_env` (plus any
    /// invocation-specific additions such as kpathsea's paranoid mode for
    /// extracted-export validation).
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutcome {
    /// `None` if the process could not be started at all (missing
    /// executable, permission denied), was killed on timeout, or was
    /// terminated by a signal.
    pub status_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub started: bool,
    /// The process exceeded its timeout and its process tree was
    /// terminated. Distinct from a crash or a nonzero exit.
    pub timed_out: bool,
}

impl ProcessOutcome {
    pub fn success() -> Self {
        Self {
            status_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
            started: true,
            timed_out: false,
        }
    }

    pub fn success_with_log(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            status_code: Some(0),
            stdout: stdout.into(),
            stderr: Vec::new(),
            started: true,
            timed_out: false,
        }
    }

    pub fn failed_to_start() -> Self {
        Self {
            status_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            started: false,
            timed_out: false,
        }
    }

    pub fn nonzero(code: i32) -> Self {
        Self {
            status_code: Some(code),
            stdout: Vec::new(),
            stderr: Vec::new(),
            started: true,
            timed_out: false,
        }
    }

    pub fn timed_out() -> Self {
        Self {
            status_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            started: true,
            timed_out: true,
        }
    }
}

/// The process boundary the engine orchestration runs against. Real builds
/// use [`RealProcessRunner`]; tests inject [`FakeProcessRunner`] to assert
/// exact argument vectors and control pass counts/failures without
/// spawning a real engine.
pub trait ProcessRunner {
    fn run(&mut self, invocation: &ProcessInvocation, timeout: Duration) -> ProcessOutcome;

    /// As [`ProcessRunner::run`], additionally delivering each stdout line
    /// to `on_line` as it arrives. The default delegates to `run` and
    /// reports nothing incrementally; only long-running provisioning
    /// commands use the streaming form.
    fn run_streaming(
        &mut self,
        invocation: &ProcessInvocation,
        timeout: Duration,
        _on_line: &mut dyn FnMut(&str),
    ) -> ProcessOutcome {
        self.run(invocation, timeout)
    }
}

static INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Marks every running and future real invocation for termination. Safe to
/// call from a signal handler: a single atomic store.
pub fn request_interrupt() {
    INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn clear_interrupt() {
    INTERRUPT_REQUESTED.store(false, Ordering::SeqCst);
}

pub fn interrupt_requested() -> bool {
    INTERRUPT_REQUESTED.load(Ordering::SeqCst)
}

/// Spawns real child processes with no shell interpolation: the executable
/// and argument vector are passed directly to the OS, never through a
/// shell. Clears the inherited environment entirely and applies exactly
/// the invocation's own list, so the environment has one owner
/// (`toolchain::prepare_child_env`). On Unix every child runs in its own
/// process group so a timeout or interrupt terminates the whole tree.
pub struct RealProcessRunner;

impl ProcessRunner for RealProcessRunner {
    fn run(&mut self, invocation: &ProcessInvocation, timeout: Duration) -> ProcessOutcome {
        run_real(invocation, timeout, None)
    }

    fn run_streaming(
        &mut self,
        invocation: &ProcessInvocation,
        timeout: Duration,
        on_line: &mut dyn FnMut(&str),
    ) -> ProcessOutcome {
        run_real(invocation, timeout, Some(on_line))
    }
}

fn run_real(
    invocation: &ProcessInvocation,
    timeout: Duration,
    mut on_line: Option<&mut dyn FnMut(&str)>,
) -> ProcessOutcome {
    let mut cmd = Command::new(&invocation.program);
    cmd.args(&invocation.args)
        .current_dir(&invocation.working_dir)
        .env_clear();
    for (k, v) in &invocation.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return ProcessOutcome::failed_to_start(),
    };

    let stdout_handle = child.stdout.take().expect("stdout was piped");
    let mut stderr_handle = child.stderr.take().expect("stderr was piped");
    let (line_tx, line_rx) = mpsc::channel::<String>();
    let stream = on_line.is_some();
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if stream {
            let mut reader = BufReader::new(stdout_handle);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        buf.extend_from_slice(&line);
                        let text = String::from_utf8_lossy(&line).trim_end().to_string();
                        let _ = line_tx.send(text);
                    }
                }
            }
        } else {
            let mut handle = stdout_handle;
            let _ = handle.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_handle.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(cb) = on_line.as_deref_mut() {
            while let Ok(line) = line_rx.try_recv() {
                cb(&line);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout || interrupt_requested() {
                    timed_out = start.elapsed() >= timeout;
                    kill_process_tree(&mut child);
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break None,
        }
    };

    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    if let Some(cb) = on_line.as_deref_mut() {
        while let Ok(line) = line_rx.try_recv() {
            cb(&line);
        }
    }

    ProcessOutcome {
        status_code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        started: true,
        timed_out,
    }
}

/// Terminates the child and everything it spawned. On Unix the child was
/// placed in its own process group, so signalling the negated group id
/// reaches grandchildren too; on Windows `taskkill /T` walks the tree.
/// `kill()` on the direct child remains the fallback in both cases.
fn kill_process_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        const SIGKILL: i32 = 9;
        let pgid = child.id() as i32;
        unsafe {
            kill(-pgid, SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Records every invocation it receives and returns pre-scripted outcomes,
/// so tests can assert exact argument vectors and drive specific
/// pass-count/failure scenarios without a real toolchain.
#[derive(Default)]
pub struct FakeProcessRunner {
    pub invocations: Vec<ProcessInvocation>,
    /// The timeout each recorded invocation was given, in the same order
    /// as `invocations`, so tests can assert every probe was bounded.
    pub timeouts: Vec<Duration>,
    responses: std::collections::VecDeque<ProcessOutcome>,
}

impl FakeProcessRunner {
    pub fn new(responses: Vec<ProcessOutcome>) -> Self {
        Self {
            invocations: Vec::new(),
            timeouts: Vec::new(),
            responses: responses.into(),
        }
    }
}

impl ProcessRunner for FakeProcessRunner {
    fn run(&mut self, invocation: &ProcessInvocation, timeout: Duration) -> ProcessOutcome {
        self.invocations.push(invocation.clone());
        self.timeouts.push(timeout);
        self.responses.pop_front().unwrap_or_else(ProcessOutcome::success)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    Xelatex,
    Biber,
}

#[derive(Debug, Clone)]
pub struct PassResult {
    pub kind: PassKind,
    pub outcome: ProcessOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileFailure {
    ToolStartFailed { program: String },
    NonZeroExit { kind: PassKind, code: Option<i32> },
    /// The pass exceeded its timeout and its process tree was terminated.
    Timeout { kind: PassKind, secs: u64 },
    PassLimitExceeded,
    /// The engine exited successfully (glyph coverage gaps are only ever
    /// a warning to xelatex's own exit code) but its log reported a
    /// character with no available glyph. Terse treats this as a build
    /// failure rather than silently shipping a document with missing
    /// text.
    MissingGlyph,
    /// The engine exited successfully and stopped asking to be rerun, but
    /// its settled log still reports unresolved references or citations.
    /// XeLaTeX writes `??` into the PDF and exits 0 in that case, so
    /// publishing it would ship visibly broken output as a success.
    UndefinedReferences,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub xelatex: PathBuf,
    pub biber: PathBuf,
    pub timeout: Duration,
}

pub const MAX_ENGINE_PASSES: u32 = 5;

fn xelatex_invocation(
    config: &EngineConfig,
    working_dir: &Path,
    main_stem: &str,
    env: &[(String, String)],
) -> ProcessInvocation {
    ProcessInvocation {
        program: config.xelatex.clone(),
        args: vec![
            "-no-shell-escape".to_string(),
            "-interaction=nonstopmode".to_string(),
            "-halt-on-error".to_string(),
            "-file-line-error".to_string(),
            "-recorder".to_string(),
            format!("{main_stem}.tex"),
        ],
        working_dir: working_dir.to_path_buf(),
        env: env.to_vec(),
    }
}

fn biber_invocation(
    config: &EngineConfig,
    working_dir: &Path,
    main_stem: &str,
    env: &[(String, String)],
) -> ProcessInvocation {
    ProcessInvocation {
        program: config.biber.clone(),
        args: vec![main_stem.to_string()],
        working_dir: working_dir.to_path_buf(),
        env: env.to_vec(),
    }
}

fn run_one_pass(
    runner: &mut dyn ProcessRunner,
    invocation: ProcessInvocation,
    kind: PassKind,
    config: &EngineConfig,
    passes: &mut Vec<PassResult>,
) -> Result<(), CompileFailure> {
    let outcome = runner.run(&invocation, config.timeout);
    let started = outcome.started;
    let timed_out = outcome.timed_out;
    let code = outcome.status_code;
    passes.push(PassResult { kind, outcome });
    if !started {
        return Err(CompileFailure::ToolStartFailed {
            program: invocation.program.display().to_string(),
        });
    }
    if timed_out {
        return Err(CompileFailure::Timeout { kind, secs: config.timeout.as_secs() });
    }
    if code != Some(0) {
        return Err(CompileFailure::NonZeroExit { kind, code });
    }
    Ok(())
}

fn rerun_needed(stdout: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stdout);
    text.contains("Rerun to get") || text.contains("Please (re)run")
}

/// Runs XeLaTeX, then Biber if `needs_biber`, then XeLaTeX again until the
/// log stops requesting a rerun, bounded to [`MAX_ENGINE_PASSES`] total
/// engine invocations, with an empty child environment. Only unit tests
/// against a fake runner use this form; real compiles go through
/// [`compile_bounded_with_env`] with a prepared environment.
pub fn compile_bounded(
    runner: &mut dyn ProcessRunner,
    working_dir: &Path,
    main_stem: &str,
    config: &EngineConfig,
    needs_biber: bool,
) -> Result<Vec<PassResult>, (Vec<PassResult>, CompileFailure)> {
    compile_bounded_with_env(runner, working_dir, main_stem, config, needs_biber, &[])
}

/// As [`compile_bounded`], with every engine invocation carrying `env` as
/// its complete environment. Returns every pass attempted so far even on
/// failure, so callers can map diagnostics back to specific tool output.
pub fn compile_bounded_with_env(
    runner: &mut dyn ProcessRunner,
    working_dir: &Path,
    main_stem: &str,
    config: &EngineConfig,
    needs_biber: bool,
    env: &[(String, String)],
) -> Result<Vec<PassResult>, (Vec<PassResult>, CompileFailure)> {
    let mut passes = Vec::new();
    let mut total_passes = 0u32;

    macro_rules! take_pass_slot {
        () => {{
            total_passes += 1;
            if total_passes > MAX_ENGINE_PASSES {
                return Err((passes, CompileFailure::PassLimitExceeded));
            }
        }};
    }

    take_pass_slot!();
    let inv = xelatex_invocation(config, working_dir, main_stem, env);
    if let Err(e) = run_one_pass(runner, inv, PassKind::Xelatex, config, &mut passes) {
        return Err((passes, e));
    }

    if needs_biber {
        take_pass_slot!();
        let inv = biber_invocation(config, working_dir, main_stem, env);
        if let Err(e) = run_one_pass(runner, inv, PassKind::Biber, config, &mut passes) {
            return Err((passes, e));
        }

        // Biber's own log never contains XeLaTeX's "Please (re)run"/"Rerun
        // to get" rerun markers (those come from biblatex during a
        // XeLaTeX pass, not from Biber itself); checking `rerun_needed`
        // against Biber's log would therefore always see "no rerun
        // needed" and skip loading the freshly written .bbl entirely,
        // leaving citations/cross-references unresolved in the published
        // PDF. A XeLaTeX pass after Biber is unconditionally required to
        // read the .bbl back in, independent of what the rerun loop below
        // decides for any further passes.
        take_pass_slot!();
        let inv = xelatex_invocation(config, working_dir, main_stem, env);
        if let Err(e) = run_one_pass(runner, inv, PassKind::Xelatex, config, &mut passes) {
            return Err((passes, e));
        }
    }

    // Even without Biber, resolving `\ref`/`\label` (or any other
    // aux-file-driven state) generally needs a second XeLaTeX pass: the
    // first pass writes the .aux file, the second reads it back. Keep
    // rerunning XeLaTeX while the log asks for it, bounded by the total
    // pass limit.
    loop {
        let last_log = &passes.last().expect("at least one pass has run").outcome.stdout;
        // The rerun marker is the engine's own hint, not the convergence
        // condition. A biblatex build reaches a pass that still reports
        // "There were undefined references" while emitting no marker, and
        // one further pass resolves it; stopping on the marker alone
        // published a PDF containing `??`. Unresolved references are
        // therefore themselves a reason to rerun, bounded by the same
        // pass budget, and only persist as a failure once that budget or
        // the document itself refuses to converge.
        if !rerun_needed(last_log) && !logs::has_undefined_references(&passes) {
            break;
        }
        take_pass_slot!();
        let inv = xelatex_invocation(config, working_dir, main_stem, env);
        if let Err(e) = run_one_pass(runner, inv, PassKind::Xelatex, config, &mut passes) {
            return Err((passes, e));
        }
    }

    Ok(passes)
}

/// Searches the host `PATH` for an executable named `name`, without
/// running it. Thin wrapper over `toolchain::find_tool_on_path` for the
/// callers that have not migrated to a resolved toolchain.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    crate::toolchain::find_tool_on_path(name, &crate::toolchain::HostEnv::capture())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_bounded_stops_on_success_without_biber() {
        let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::success()]);
        let config = EngineConfig {
            xelatex: PathBuf::from("xelatex"),
            biber: PathBuf::from("biber"),
            timeout: Duration::from_secs(5),
        };
        let result = compile_bounded(&mut runner, Path::new("/tmp/work"), "paper", &config, false);
        assert!(result.is_ok());
        assert_eq!(runner.invocations.len(), 1);
    }

    #[test]
    fn test_compile_bounded_runs_biber_and_converges() {
        let mut runner = FakeProcessRunner::new(vec![
            ProcessOutcome::success(),                                       // xelatex pass 1
            ProcessOutcome::success_with_log(&b"Please (re)run"[..]),        // biber (requests a rerun)
            ProcessOutcome::success_with_log(&b"Rerun to get it right"[..]), // xelatex pass 3
            ProcessOutcome::success(),                                       // xelatex pass 4 (converged)
        ]);
        let config = EngineConfig {
            xelatex: PathBuf::from("xelatex"),
            biber: PathBuf::from("biber"),
            timeout: Duration::from_secs(5),
        };
        let result = compile_bounded(&mut runner, Path::new("/tmp/work"), "paper", &config, true);
        assert!(result.is_ok());
        assert_eq!(runner.invocations.len(), 4);
    }

    #[test]
    fn test_timeout_yields_timeout_variant_not_nonzero_exit() {
        let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::timed_out()]);
        let config = EngineConfig {
            xelatex: PathBuf::from("xelatex"),
            biber: PathBuf::from("biber"),
            timeout: Duration::from_secs(60),
        };
        let err = compile_bounded(&mut runner, Path::new("/tmp/work"), "paper", &config, false).unwrap_err();
        assert_eq!(err.1, CompileFailure::Timeout { kind: PassKind::Xelatex, secs: 60 });
    }

    #[test]
    fn test_real_runner_applies_only_invocation_env() {
        #[cfg(unix)]
        {
            let invocation = ProcessInvocation {
                program: PathBuf::from("/usr/bin/env"),
                args: vec![],
                working_dir: std::env::temp_dir(),
                env: vec![("ONLY_KEY".to_string(), "only-value".to_string())],
            };
            let outcome = RealProcessRunner.run(&invocation, Duration::from_secs(5));
            assert!(outcome.started);
            let text = String::from_utf8_lossy(&outcome.stdout);
            let keys: Vec<&str> = text.lines().filter_map(|l| l.split('=').next()).collect();
            assert_eq!(keys, vec!["ONLY_KEY"], "nothing inherited, nothing restored: {text}");
        }
    }
}
