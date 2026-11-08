// vault 밖으로 나가는 파일 작업: 휴지통, 백업. 지금 앱과 GPUI 앱이 같은 규칙(허용 경로, 백업 폴더 이름)을 쓴다.

use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::ai::AiError;

/// appDataDir 기준 상대경로를 macOS 휴지통으로 이동(영구 삭제 대신 복구 가능).
/// vault 밖 경로는 거부한다. 대상이 없으면 멱등 성공.
/// 블로킹 함수다 — `trash` 의 macOS 기본 경로는 Finder 에 AppleEvent 를 보내 응답까지 기다린다.
/// 부르는 쪽이 워커 스레드에서 돌린다.
/// rel_path 는 base(appdata) 상대경로(기본 보관함) 또는 절대경로("폴더 열기"로 연 워크스페이스).
pub fn move_to_trash(base: &Path, home: &Path, rel_path: &str) -> Result<(), AiError> {
    let p = PathBuf::from(rel_path);
    let target = if p.is_absolute() { p } else { base.join(&p) };
    if !target.exists() {
        return Ok(());
    }
    let io = |e: std::io::Error| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string());
    let canon_target = target.canonicalize().map_err(io)?;
    let canon_base = base.canonicalize().map_err(io)?;
    let canon_home = home.canonicalize().map_err(io)?;
    // 허용: appdata 하위, 또는 홈 하위(홈 자체는 금지) — 시스템 경로 오삭제 방지
    let allowed = canon_target.starts_with(&canon_base)
        || (canon_target.starts_with(&canon_home) && canon_target != canon_home);
    if !allowed {
        return Err(AiError::new("TRASH_FORBIDDEN", "허용되지 않은 경로입니다."));
    }
    trash::delete(&canon_target)
        .map_err(|e| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string()))
}

/// 백업: 사용자가 고른 폴더 아래 `amber-backup-<로컬 타임스탬프>/` 를 만들고
/// DB 스냅샷(amber.db)과 vault/ 사본을 넣는다. 성공 시 만들어진 폴더의 절대경로를 돌려준다.
/// tz_offset_min 은 JS Date.getTimezoneOffset() 값(report.rs 와 같은 부호 규약).
/// db = amber.db 경로, data_dir = vault 가 들어 있는 앱 데이터 폴더.
pub async fn create_backup(
    db: PathBuf,
    data_dir: PathBuf,
    dest_dir: String,
    tz_offset_min: i32,
    // "폴더 열기"로 바꾼 작업 폴더들(섹션명, 절대경로). 기본 보관함이면 프론트가 넘기지 않는다.
    // 이걸 안 받으면 노트를 git 으로 관리하려고 폴더를 연 사용자가 노트 0개짜리 백업을 받는다.
    extra_roots: Option<Vec<(String, String)>>,
) -> Result<String, AiError> {
    let vault = data_dir.join("vault");

    let dest = std::path::PathBuf::from(dest_dir.trim());
    if !dest.is_dir() {
        return Err(AiError::new("BACKUP_NO_DEST", "백업할 폴더를 찾을 수 없습니다."));
    }
    // 백업 대상이 앱 데이터 폴더 안이면 vault 를 자기 자신 안으로 무한 복사하게 된다
    if let (Ok(d), Ok(base)) = (dest.canonicalize(), data_dir.canonicalize()) {
        if d.starts_with(&base) {
            return Err(AiError::new(
                "BACKUP_INSIDE_APPDATA",
                "앱 데이터 폴더 밖의 위치를 선택해 주세요.",
            ));
        }
    }

    // 같은 폴더에 여러 번 백업해도 덮어쓰지 않게 하위 폴더로 나눈다
    // (VACUUM INTO 는 대상 파일이 이미 있으면 실패한다).
    let root = make_backup_dir(&dest, &local_stamp(tz_offset_min))
        .map_err(|e| AiError::detailed("BACKUP_MKDIR", e.to_string(), e.to_string()))?;

    let roots = extra_roots.unwrap_or_default();
    let write = async {
        if db.exists() {
            vacuum_into(&db, &root.join("amber.db")).await?;
        }
        if vault.is_dir() {
            let (from, to) = (vault.clone(), root.join("vault"));
            // 파일 복사는 블로킹이라 워커 스레드로 — 그동안 UI(IPC)가 멈추지 않게
            tokio::task::spawn_blocking(move || copy_dir(&from, &to))
                .await
                .map_err(|e| e.to_string())??;
        }
        // 커스텀 작업 폴더는 roots/<섹션>/ 아래로. 어디서 왔는지는 manifest.json 에 남긴다.
        // 실제로 복사한 것만 모아 마지막에 한 번 직렬화한다 — JSON 을 손으로 이어 붙이면
        // 건너뛴 항목이 쉼표 자리를 어긋내고(첫 루트가 빠지면 `{,` 가 된다) 섹션 이름 이스케이프도 빠진다.
        let mut copied = serde_json::Map::new();
        for (section, path) in roots.iter() {
            let src = PathBuf::from(path);
            if !src.is_dir() {
                continue;
            }
            // 백업 대상 폴더가 이 루트 안이면 자기 자신을 재귀 복사하게 된다
            if let (Ok(d), Ok(r)) = (dest.canonicalize(), src.canonicalize()) {
                if d.starts_with(&r) {
                    return Err(format!(
                        "백업 위치가 '{section}' 작업 폴더 안이라 복사할 수 없습니다."
                    ));
                }
            }
            let to = root.join("roots").join(section);
            tokio::task::spawn_blocking(move || copy_dir(&src, &to))
                .await
                .map_err(|e| e.to_string())??;
            copied.insert(section.clone(), serde_json::Value::String(path.clone()));
        }
        if !roots.is_empty() {
            let manifest = serde_json::json!({ "version": 1, "roots": copied });
            let body = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
            std::fs::write(root.join("manifest.json"), body + "\n").map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    };

    if let Err(e) = write.await {
        // 반쪽짜리 폴더를 남기면 온전한 백업으로 오인한다 — 방금 만든 폴더만 되돌린다
        let _ = std::fs::remove_dir_all(&root);
        // vacuum_into/copy_dir 의 사유는 그대로 detail 로 넘긴다(프론트가 코드로 문구를 만든다)
        return Err(AiError::detailed("BACKUP_WRITE", e.clone(), e));
    }
    Ok(root.to_string_lossy().into_owned())
}

/// 백업 폴더를 **새로** 만든다. 이름이 이미 있으면 `-2`, `-3` … 으로 비켜 간다.
///
/// `create_dir_all` 을 쓰면 안 된다 — 이미 있는 폴더에도 `Ok` 를 준다. 그래서 같은 초에 두 번
/// 백업하면 앞선 백업 폴더를 그대로 재사용하고, `VACUUM INTO` 가 "대상 파일이 이미 있다"로 실패한 뒤
/// 실패 정리(`remove_dir_all`)가 **먼저 성공한 남의 백업을 통째로 지운다**. 서버가 없는 앱에서
/// 백업은 유일한 복구 수단이라, 그걸 지우는 경로는 남겨둘 수 없다.
/// 반환된 경로는 이 호출이 직접 만든 것이므로 실패 시 지워도 안전하다.
fn make_backup_dir(dest: &Path, stamp: &str) -> std::io::Result<PathBuf> {
    for n in 1..=99 {
        let name = if n == 1 {
            format!("amber-backup-{stamp}")
        } else {
            format!("amber-backup-{stamp}-{n}")
        };
        let p = dest.join(name);
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "같은 시각의 백업 폴더가 너무 많습니다.",
    ))
}

/// DB 스냅샷. journal_mode 가 WAL 이라 amber.db 파일 복사는 최근 쓰기를 놓치므로,
/// 앱이 DB 를 연 채로도 일관된 사본을 만드는 `VACUUM INTO` 를 쓴다.
/// sqlx 를 직접 의존하지 않으므로 macOS 기본 제공 sqlite3 CLI 로 실행한다.
async fn vacuum_into(db: &Path, out: &Path) -> Result<(), String> {
    // 경로는 SQL 문자열 리터럴로 들어가므로 작은따옴표만 이스케이프(셸은 거치지 않는다)
    let sql = format!(
        "VACUUM INTO '{}'",
        out.to_string_lossy().replace('\'', "''")
    );
    let res = timeout(
        Duration::from_secs(120),
        Command::new("/usr/bin/sqlite3")
            .arg(db)
            .arg(&sql)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "DB 백업이 시간 안에 끝나지 않았습니다.".to_string())?;

    let output = res.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "시스템 sqlite3 를 찾을 수 없어 DB 를 백업하지 못했습니다.".to_string()
        } else {
            format!("sqlite3 실행 실패: {e}")
        }
    })?;
    if !output.status.success() {
        return Err(format!(
            "DB 백업 실패: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// 재귀 복사 — tauri-plugin-fs 에 재귀 복사가 없어 직접 구현.
/// 심볼릭 링크는 건너뛴다(백업본이 원본 밖을 가리키는 것 방지).
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let dst = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &dst)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 백업 폴더 이름용 로컬 타임스탬프(YYYYMMDD-HHMMSS). chrono 없이 epoch 초에서 직접 환산.
/// tz_offset_min = JS getTimezoneOffset()(UTC 기준 분, KST=-540) — report.rs fmt_hhmm 과 동일 산술.
fn local_stamp(tz_offset_min: i32) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    stamp_from(now, tz_offset_min)
}

/// 시계를 뺀 순수부 — 이 이름이 백업 폴더를 가리므로 월말·윤년·타임존 경계가 어긋나면
/// 백업을 식별할 수 없게 되고, 최악의 경우 이름이 반복 충돌한다. 그래서 따로 테스트한다.
fn stamp_from(epoch_secs: i64, tz_offset_min: i32) -> String {
    let secs = epoch_secs - i64::from(tz_offset_min) * 60;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // 3월을 연초로 두는 민력(civil) 역산 — 윤년 분기를 한 번에 처리한다
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = era * 400 + yoe + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 백업 폴더 이름은 초 단위라 같은 초에 두 번 누르면 충돌한다. 그때 **기존 폴더를
    /// 재사용하면 안 된다** — 재사용은 VACUUM INTO 실패 → 실패 정리 → 남의 백업 삭제로 이어졌다.
    #[test]
    fn make_backup_dir_never_reuses_an_existing_folder() {
        let dest = std::env::temp_dir().join(format!("amber-mkdir-{}", std::process::id()));
        std::fs::create_dir_all(&dest).unwrap();

        let a = make_backup_dir(&dest, "20260911-160000").unwrap();
        let b = make_backup_dir(&dest, "20260911-160000").unwrap();
        assert_ne!(a, b, "같은 스탬프인데 같은 폴더를 돌려줬다");
        assert!(a.ends_with("amber-backup-20260911-160000"));
        assert!(b.ends_with("amber-backup-20260911-160000-2"));
        assert!(a.is_dir() && b.is_dir());

        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn stamp_from_applies_timezone_offset() {
        // KST = UTC+9 → getTimezoneOffset() 은 -540 (report.rs fmt_hhmm 과 같은 부호 규약)
        assert_eq!(stamp_from(1_767_193_200, -540), "20260101-000000");
        assert_eq!(stamp_from(1_767_193_200, 0), "20251231-150000");
    }

    #[test]
    fn stamp_from_crosses_leap_day_and_year_end() {
        // 2024-02-29T15:00:00Z + KST → 2024-03-01 00:00 (윤년 2월 29일 다음날)
        assert_eq!(stamp_from(1_709_218_800, -540), "20240301-000000");
        // 2026-12-31T15:00:00Z + KST → 2027-01-01 00:00 (연 경계)
        assert_eq!(stamp_from(1_798_729_200, -540), "20270101-000000");
    }

    #[test]
    fn stamp_from_is_zero_padded() {
        // 1970-01-02T03:04:05Z, UTC — 한 자리 월/일/시가 두 자리로 채워져야 정렬이 유지된다
        assert_eq!(stamp_from(97_445, 0), "19700102-030405");
    }
}
