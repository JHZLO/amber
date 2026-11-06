//! ELK 배치 — elkjs(ELK 를 GWT 로 옮긴 것)를 macOS 내장 JavaScriptCore 로 돌린다. 지금 앱(WKWebView)과 같은 엔진, 같은 코드라
//! 배치가 똑같다. merman 의 ELK 이식은 포트 입력이 없어 resin 의 행/카드 위아래 포트 배치를 못 낸다(사용자 결정, PLAN D4).
//!
//! 워커 스크립트는 `self` 가 있고 `document` 가 없으면 `self.onmessage` 를 건다. 'layout' 명령은 그 자리에서 끝나고
//! `self.postMessage` 로 답한다 — 이벤트 루프 없이 부르고 바로 읽는다. 보내는 말은 elk-api 와 같다(먼저 'register').
//! 문맥 하나를 잠금 뒤에 두고 처음 부를 때 만든다(스크립트 1.6MB 를 한 번 읽는다). 부르는 쪽이 뒤 스레드에서 부른다.

use std::ffi::{CString, c_char, c_int, c_void};
use std::sync::Mutex;

const WORKER: &str = include_str!("../vendor/elkjs/elk-worker.min.js");

/// elk-api 가 처음에 등록하는 알고리즘(ELK 생성자의 기본값)
const REGISTER: &str = "self.onmessage({data:{id:0,cmd:'register',algorithms:['layered','stress','mrtree','radial','force','disco','sporeOverlap','sporeCompaction','rectpacking']}});";

type Ctx = *mut c_void;
type JsString = *mut c_void;
type JsValue = *const c_void;

#[link(name = "JavaScriptCore", kind = "framework")]
unsafe extern "C" {
    fn JSGlobalContextCreate(global_class: *mut c_void) -> Ctx;
    fn JSStringCreateWithUTF8CString(s: *const c_char) -> JsString;
    fn JSStringRelease(s: JsString);
    fn JSStringGetMaximumUTF8CStringSize(s: JsString) -> usize;
    fn JSStringGetUTF8CString(s: JsString, buf: *mut c_char, size: usize) -> usize;
    fn JSEvaluateScript(
        ctx: Ctx,
        script: JsString,
        this: *mut c_void,
        url: JsString,
        line: c_int,
        exception: *mut JsValue,
    ) -> JsValue;
    fn JSValueToStringCopy(ctx: Ctx, value: JsValue, exception: *mut JsValue) -> JsString;
}

struct Engine(Ctx);

// JavaScriptCore 는 문맥을 쓰는 동안 제 잠금(JSLock)을 잡는다 — 여기선 바깥 Mutex 로 한 번에 하나만 쓴다
unsafe impl Send for Engine {}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

fn take_string(s: JsString) -> String {
    unsafe {
        let cap = JSStringGetMaximumUTF8CStringSize(s);
        let mut buf = vec![0u8; cap.max(1)];
        let n = JSStringGetUTF8CString(s, buf.as_mut_ptr() as *mut c_char, cap);
        JSStringRelease(s);
        // n 은 끝의 NUL 까지 센다
        buf.truncate(n.saturating_sub(1));
        String::from_utf8_lossy(&buf).into_owned()
    }
}

impl Engine {
    fn new() -> Result<Self, String> {
        let ctx = unsafe { JSGlobalContextCreate(std::ptr::null_mut()) };
        if ctx.is_null() {
            return Err("JavaScriptCore 문맥을 만들지 못했다".into());
        }
        let e = Engine(ctx);
        // 워커처럼 보이게 — postMessage 는 답을 받아 둔다
        e.eval(
            "var self = this; var __out = null; self.postMessage = function (m) { __out = m; };",
        )?;
        e.eval(WORKER)?;
        e.eval(REGISTER)?;
        Ok(e)
    }

    fn eval(&self, script: &str) -> Result<String, String> {
        let src = CString::new(script).map_err(|_| "스크립트에 NUL 이 있다".to_string())?;
        unsafe {
            let s = JSStringCreateWithUTF8CString(src.as_ptr());
            let mut exc: JsValue = std::ptr::null();
            let v = JSEvaluateScript(
                self.0,
                s,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut exc,
            );
            JSStringRelease(s);
            if !exc.is_null() {
                let mut e2: JsValue = std::ptr::null();
                let msg = JSValueToStringCopy(self.0, exc, &mut e2);
                return Err(if msg.is_null() {
                    "JS 오류".into()
                } else {
                    take_string(msg)
                });
            }
            if v.is_null() {
                return Ok(String::new());
            }
            let mut e2: JsValue = std::ptr::null();
            let out = JSValueToStringCopy(self.0, v, &mut e2);
            Ok(if out.is_null() {
                String::new()
            } else {
                take_string(out)
            })
        }
    }
}

/// ELK JSON 그래프를 배치해 배치된 그래프 JSON 을 돌려준다(elk.layout(graph) 와 같은 결과)
pub fn layout(graph_json: &str) -> Result<String, String> {
    let mut guard = ENGINE.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        *guard = Some(Engine::new()?);
    }
    let e = guard.as_ref().expect("방금 만들었다");
    // JSON 은 그대로 JS 식이다. elk-api 는 layoutOptions 에 기본값({}), options 에 로그/시간 끔을 보낸다
    let script = format!(
        "__out = null; self.onmessage({{data:{{id:1,cmd:'layout',graph:{graph_json},layoutOptions:{{}},options:{{logging:false,measureExecutionTime:false}}}}}});\
         __out && __out.error ? '\\u0000' + String(__out.error && (__out.error.message || __out.error)) : JSON.stringify(__out && __out.data)"
    );
    let out = e.eval(&script)?;
    match out.strip_prefix('\u{0}') {
        Some(err) => Err(err.to_string()),
        None => Ok(out),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    /// 두 배치가 같은가 — 같은 코드라 수까지 같아야 한다(엔진의 수학 함수 끝자리만 봐준다)
    fn same(a: &Value, b: &Value, path: &str) -> Result<(), String> {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                if (x - y).abs() > 1e-9 * x.abs().max(1.) {
                    return Err(format!("{path}: {x} != {y}"));
                }
                Ok(())
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    return Err(format!("{path}: 길이 {} != {}", x.len(), y.len()));
                }
                x.iter()
                    .zip(y)
                    .enumerate()
                    .try_for_each(|(i, (p, q))| same(p, q, &format!("{path}[{i}]")))
            }
            // $H 는 GWT 가 객체에 붙이는 일련번호(identity hash) — 앞서 몇 번 배치했느냐에 따라 달라지고 resin 은 읽지 않는다
            (Value::Object(x), Value::Object(y)) => {
                let mut kx: Vec<_> = x.keys().filter(|k| *k != "$H").collect();
                let mut ky: Vec<_> = y.keys().filter(|k| *k != "$H").collect();
                kx.sort();
                ky.sort();
                if kx != ky {
                    return Err(format!("{path}: 키 {kx:?} != {ky:?}"));
                }
                kx.iter()
                    .try_for_each(|k| same(&x[*k], &y[*k], &format!("{path}.{k}")))
            }
            _ if a == b => Ok(()),
            _ => Err(format!("{path}: {a} != {b}")),
        }
    }

    #[test]
    fn lays_out_like_elkjs_in_node() {
        let cases: Value = serde_json::from_str(include_str!("../tests/resin_elk.json")).unwrap();
        let mut n = 0;
        for c in cases.as_array().unwrap() {
            for l in c["layouts"].as_array().unwrap() {
                let out = super::layout(&l["graph"].to_string()).unwrap();
                let out: Value = serde_json::from_str(&out).unwrap();
                if let Err(e) = same(&out, &l["out"], "") {
                    panic!("{}: {e}", c["name"]);
                }
                n += 1;
            }
        }
        assert_eq!(n, 8);
    }

    #[test]
    fn reports_elk_errors() {
        let e = super::layout(
            r#"{"id":"root","children":[{"id":"a"}],"edges":[{"id":"e","sources":["a"],"targets":["nope"]}]}"#,
        );
        assert!(e.is_err(), "{e:?}");
    }
}
