// 사용자 셸의 PATH — 앱이 띄우는 모든 CLI(gh, claude, codex)가 이 값으로 실행된다.
//
// Dock, Finder, 로그인 항목으로 뜬 앱은 셸을 거치지 않아 launchd 기본 PATH(/usr/bin:/bin:…)만 받는다.
// Homebrew, nix, ~/.local/bin 에 깔린 CLI 는 거기 없다 — 터미널에선 되는데 앱에선 "없다"가 된다.
// 그래서 사용자 로그인 셸을 한 번 띄워 PATH 를 읽고(세션당 한 번, 캐시), 자식 프로세스마다 넣는다.
// 프로세스 전역 env 를 바꾸지 않는 이유: 다른 스레드(WebKit 등)가 getenv 하는 중에 setenv 하면 안전하지 않다.

use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::OnceCell;
use tokio::time::timeout;

const MARK: &str = "__AMBER_PATH__";
/// 셸 설정이 무거우면(oh-my-zsh 등) 1초를 넘기도 한다. 막히면 폴백으로 간다
const SHELL_TIMEOUT_SECS: u64 = 6;

static PATH: OnceCell<String> = OnceCell::const_new();

/// 앱 시작 직후 미리 읽어 둔다 — 첫 CLI 호출이 셸 기동을 기다리지 않게
pub fn prewarm() {
    tauri::async_runtime::spawn(async {
        path().await;
    });
}

/// 자식 프로세스에 줄 PATH (셸 PATH + 지금 PATH + 흔한 설치 위치)
pub async fn path() -> &'static str {
    PATH.get_or_init(load).await
}

/// `Command::new` 대신 쓴다 — PATH 를 넣어 두면 맨 이름("gh")도 그 PATH 에서 찾는다
pub async fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(resolve_program(program.as_ref()).await);
    cmd.env("PATH", path().await);
    cmd
}

/// 맨 이름이면 셸 PATH 에서 절대경로로 바꾼다. 못 찾으면 그대로(실행 시 NotFound 로 드러난다)
async fn resolve_program(program: &OsStr) -> std::ffi::OsString {
    match program.to_str() {
        Some(p) if !p.contains('/') => which(p).await.map(Into::into).unwrap_or_else(|| program.into()),
        _ => program.into(),
    }
}

/// 실행 파일 찾기. 경로를 주면 그 파일이 있는지만, 이름이면 PATH 를 훑는다
pub async fn which(bin: &str) -> Option<String> {
    let bin = bin.trim();
    if bin.is_empty() {
        return None;
    }
    if bin.contains('/') {
        return is_executable(Path::new(bin)).then(|| bin.to_string());
    }
    find_in(path().await, bin)
}

fn find_in(path: &str, bin: &str) -> Option<String> {
    path.split(':')
        .filter(|d| !d.is_empty())
        .map(|d| Path::new(d).join(bin))
        .find(|p| is_executable(p))
        .map(|p| p.to_string_lossy().into_owned())
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

async fn load() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let from_shell = read_shell_path().await;
    merge_path(from_shell.as_deref(), &current, &fallback_dirs(&home))
}

/// 셸이 PATH 를 못 줘도(타임아웃, 낯선 셸) 여기는 본다 — 맥에서 CLI 가 흔히 깔리는 곳
fn fallback_dirs(home: &str) -> Vec<String> {
    let mut v: Vec<String> = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/opt/local/bin",
        "/nix/var/nix/profiles/default/bin",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if !home.is_empty() {
        for rel in [".local/bin", ".nix-profile/bin", ".cargo/bin", ".bun/bin", ".volta/bin"] {
            v.push(format!("{home}/{rel}"));
        }
    }
    v
}

/// 사용자 셸(`$SHELL`)을 로그인+대화형으로 띄워 PATH 를 받는다. `.zprofile` 과 `.zshrc` 를 모두 읽어야
/// 터미널과 같은 값이 나온다(-l 만으로는 `.zshrc` 를 안 읽는다). 대화형을 못 받는 셸이면 -l 만, 그다음 zsh.
async fn read_shell_path() -> Option<String> {
    let user_shell = std::env::var("SHELL")
        .ok()
        .filter(|s| s.starts_with('/') && Path::new(s).is_file());
    let mut tries: Vec<(String, &str)> = Vec::new();
    if let Some(s) = &user_shell {
        tries.push((s.clone(), "-ilc"));
        tries.push((s.clone(), "-lc"));
    }
    if user_shell.as_deref() != Some("/bin/zsh") {
        tries.push(("/bin/zsh".into(), "-ilc"));
    }
    // printenv 는 셸 문법과 무관하게 콜론으로 이은 PATH 를 찍는다(fish 의 $PATH 는 공백 목록이다)
    let script = format!("printf '%s' {MARK}; /usr/bin/printenv PATH; printf '%s' {MARK}");
    for (shell, flags) in tries {
        let run = Command::new(&shell)
            .args([flags, &script])
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output();
        if let Ok(Ok(out)) = timeout(Duration::from_secs(SHELL_TIMEOUT_SECS), run).await {
            if let Some(p) = parse_marked(&String::from_utf8_lossy(&out.stdout)) {
                return Some(p);
            }
        }
    }
    None
}

/// 표식 사이의 PATH 만 꺼낸다 — 셸 설정이 인사말이나 경고를 찍어도 섞이지 않게
fn parse_marked(stdout: &str) -> Option<String> {
    let start = stdout.find(MARK)? + MARK.len();
    let end = start + stdout[start..].find(MARK)?;
    let p = stdout[start..end].trim();
    (!p.is_empty() && p.contains('/')).then(|| p.to_string())
}

/// 셸 PATH → 지금 PATH → 폴백 순서로 잇고 겹치는 건 앞의 것만 남긴다.
/// 폴백은 실제로 있는 폴더만 — 없는 경로가 줄줄이 붙으면 디버깅할 때 읽기만 어렵다.
fn merge_path(shell: Option<&str>, current: &str, fallback: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut push = |d: &str| {
        let d = d.trim();
        if !d.is_empty() && !out.iter().any(|x| x == d) {
            out.push(d.to_string());
        }
    };
    shell.unwrap_or("").split(':').for_each(&mut push);
    current.split(':').for_each(&mut push);
    for d in fallback {
        if Path::new(d).is_dir() {
            push(d);
        }
    }
    out.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_marked_ignores_shell_noise_around_the_path() {
        let out = format!("Welcome!\n{MARK}/opt/homebrew/bin:/usr/bin\n{MARK}\nbye");
        assert_eq!(parse_marked(&out).as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(parse_marked("no marks here"), None);
        assert_eq!(parse_marked(&format!("{MARK}{MARK}")), None);
    }

    #[test]
    fn merge_path_puts_the_shell_first_and_drops_duplicates() {
        let merged = merge_path(Some("/opt/homebrew/bin:/usr/bin"), "/usr/bin:/bin", &[]);
        assert_eq!(merged, "/opt/homebrew/bin:/usr/bin:/bin");
    }

    #[test]
    fn merge_path_keeps_only_fallback_dirs_that_exist() {
        let merged = merge_path(None, "/usr/bin", &["/bin".into(), "/definitely/not/here".into()]);
        assert_eq!(merged, "/usr/bin:/bin");
    }

    #[test]
    fn find_in_returns_the_first_executable_hit() {
        assert_eq!(find_in("/nope:/bin", "sh").as_deref(), Some("/bin/sh"));
        assert_eq!(find_in("/nope", "sh"), None);
    }
}
