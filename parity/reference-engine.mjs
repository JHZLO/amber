#!/usr/bin/env node
// 외부 엔진 계약(engine.ts ExternalEngine)의 참조 구현. vacations.* 네 함수만 할 줄 안다 — 프로토콜,
// 시계 고정, op 마다 커밋, migrate 장부를 하네스가 제대로 재는지 확인하는 데 쓴다(parity.test.ts).
// Rust 엔진은 이 파일과 같은 대화를 하면 된다.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..");
let now = 0;
let db = null;

function open(path) {
  const conn = new DatabaseSync(path);
  conn.exec("PRAGMA foreign_keys = ON");
  conn.function("unixepoch", { varargs: true, deterministic: false }, (...a) => {
    if (a.length === 1 && a[0] === "subsec") return now / 1000;
    throw new Error(`unixepoch(${a.join(", ")})`);
  });
  return conn;
}

const KINDS = ["annual", "half", "sick", "public", "special"];
const kind = (k) => (KINDS.includes(k) ? k : "annual");

const OPS = {
  "vacations.listVacations": (from, to) => {
    const out = {};
    for (const r of db.prepare("SELECT date, kind FROM vacations WHERE date BETWEEN ? AND ?").all(from, to)) {
      out[r.date] = kind(r.kind);
    }
    return out;
  },
  "vacations.getVacation": (date) => {
    const r = db.prepare("SELECT kind FROM vacations WHERE date = ?").get(date);
    return r ? kind(r.kind) : null;
  },
  "vacations.setVacation": (date, k) => {
    db.prepare("INSERT INTO vacations (date, kind) VALUES (?1, ?2) ON CONFLICT(date) DO UPDATE SET kind = ?2, updated_at = ?3").run(
      date,
      k,
      now,
    );
    return null;
  },
  "vacations.removeVacation": (date) => {
    db.prepare("DELETE FROM vacations WHERE date = ?").run(date);
    return null;
  },
};

/** sqlx 처럼 장부를 남기며 마이그레이션을 깐다 — 장부 모양은 sqlx 0.8 의 _sqlx_migrations */
function migrate(path) {
  const conn = open(path);
  conn.exec(`CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)`);
  const lib = readFileSync(join(REPO, "src-tauri/src/lib.rs"), "utf8");
  const re = /version:\s*(\d+)\s*,\s*description:\s*"([^"]*)"\s*,\s*sql:\s*include_str!\(\s*"\.\.\/migrations\/([^"]+)"\s*\)/g;
  for (const [, version, description, file] of [...lib.matchAll(re)].sort((a, b) => a[1] - b[1])) {
    const sql = readFileSync(join(REPO, "src-tauri/migrations", file), "utf8");
    conn.exec(sql);
    conn
      .prepare("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, ?, 1, ?, 0)")
      .run(Number(version), description, createHash("sha384").update(sql).digest());
  }
  conn.close();
}

const reply = (msg) => process.stdout.write(JSON.stringify(msg) + "\n");

for await (const line of createInterface({ input: process.stdin })) {
  const m = JSON.parse(line);
  try {
    if (m.cmd === "open") {
      process.env.TZ = m.tz;
      db = open(m.db);
      reply({ ok: true });
    } else if (m.cmd === "op") {
      now = m.now;
      const f = OPS[m.op];
      if (!f) throw new Error(`모르는 op: ${m.op}`);
      reply({ ok: true, result: f(...m.args) });
    } else if (m.cmd === "close") {
      db?.close();
      db = null;
      reply({ ok: true });
    } else if (m.cmd === "migrate") {
      migrate(m.db);
      reply({ ok: true });
    } else if (m.cmd === "ops") {
      reply({ ok: true, ops: Object.keys(OPS), migrate: true });
    } else {
      throw new Error(`모르는 cmd: ${m.cmd}`);
    }
  } catch (e) {
    reply({ ok: false, error: String(e?.message ?? e) });
  }
}
