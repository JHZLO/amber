// 엔진 = 도메인 함수를 실제로 돌리는 쪽. 지금 TS 구현(TsEngine)이 정답을 녹화하고, 옮긴 구현은
// ExternalEngine 으로 붙어 같은 픽스처를 재생한다. 상태는 하네스가 자기 연결로 직접 뜬다 —
// 엔진이 "이렇게 바뀌었다"고 말하는 걸 믿지 않는다.

import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createInterface, type Interface } from "node:readline";
import { vi } from "vitest";
import { callOp } from "./ops";
import { clock, openSqlite, pluginSql, ulids } from "./sqlite";

export type Outcome = { ok: true; result: unknown } | { ok: false; error: string };

export interface Engine {
  readonly name: string;
  /** 하네스가 마이그레이션과 시드까지 깐 파일을 연다. 엔진은 마이그레이션을 돌리지 않는다 */
  open(dbPath: string, tz: string): Promise<void>;
  /** now 는 앱 시계와 SQLite unixepoch('subsec') 둘 다다. 답하기 전에 커밋한다 */
  run(op: string, args: unknown[], now: number): Promise<Outcome>;
  close(): Promise<void>;
  /** 빈 파일에 자기 마이그레이션을 깐다 — 기존 amber.db 를 그대로 열 수 있는지 재는 데 쓴다 */
  migrate?(dbPath: string): Promise<void>;
}

/** JSON 으로 오가는 값만 남긴다 — undefined 키는 빠지고, Rust 쪽 직렬화와 같은 모양이 된다 */
export const jsonOnly = (v: unknown): unknown => (v === undefined ? null : JSON.parse(JSON.stringify(v)));

/** 지금 앱의 src/lib 함수를 그대로 돌린다. plugin-sql 과 ulid 는 parity.test.ts 가 바꿔 끼운다 */
export class TsEngine implements Engine {
  readonly name = "ts";

  async open(dbPath: string, tz: string) {
    process.env.TZ = tz;
    pluginSql.conn = openSqlite(dbPath);
    ulids.k = 0;
  }

  async run(op: string, args: unknown[], now: number): Promise<Outcome> {
    clock.now = now;
    vi.setSystemTime(now);
    try {
      return { ok: true, result: jsonOnly(await callOp(op, structuredClone(args))) };
    } catch (e) {
      return { ok: false, error: String((e as Error)?.message ?? e) };
    }
  }

  async close() {
    pluginSql.conn?.close();
    pluginSql.conn = null;
  }
}

/** 다른 프로세스의 엔진(Rust 포트). 표준 입출력으로 한 줄에 JSON 하나씩 주고받는다:
 *
 *    → {"cmd":"open","db":"/tmp/…/amber.db","tz":"Asia/Seoul"}             ← {"ok":true}
 *    → {"cmd":"op","op":"todos.createTodo","args":[…],"now":1793836800000}  ← {"ok":true,"result":…}
 *                                                                           ← {"ok":false,"error":"…"}
 *    → {"cmd":"close"}                                                      ← {"ok":true}
 *    → {"cmd":"migrate","db":"/tmp/…/fresh.db"}                             ← {"ok":true}
 *    → {"cmd":"ops"}                                ← {"ok":true,"ops":["todos.listTodos",…],"migrate":false}
 *
 *  옮기는 중인 엔진은 ops 로 아는 함수만 밝힌다. 하네스는 모르는 함수가 든 픽스처를 건너뛰고(실패 아님)
 *  건너뛴 이유를 남긴다 — 다 옮기면 건너뛰는 픽스처가 없어야 끝이다.
 *
 *  지킬 것:
 *   - open 에서 마이그레이션을 돌리지 않는다. 하네스가 이미 깔아 뒀다
 *   - op 마다 커밋하고 답한다. 하네스가 다른 연결로 바로 상태를 뜬다
 *   - now 는 앱의 시계이자 SQLite 의 unixepoch('subsec') 이다. 앱 정의 함수로 내장 함수를 덮는다
 *   - ULID 는 open 마다 처음부터 parityUlid(1), parityUlid(2)… 순서로 낸다(sqlite.ts)
 *   - 결과 JSON 은 지금 TS 함수가 돌려주는 모양 그대로다(필드 이름, null, 배열 순서까지) */
export class ExternalEngine implements Engine {
  readonly name: string;
  private proc: ChildProcessWithoutNullStreams | null = null;
  private lines: Interface | null = null;
  private waiting: ((line: string) => void)[] = [];

  constructor(private command: string) {
    this.name = `external(${command})`;
  }

  private start() {
    if (this.proc) return;
    this.proc = spawn(this.command, { shell: true, stdio: ["pipe", "pipe", "pipe"] });
    this.proc.stderr.on("data", (d) => process.stderr.write(d));
    this.lines = createInterface({ input: this.proc.stdout });
    this.lines.on("line", (line) => this.waiting.shift()?.(line));
  }

  private send(msg: Record<string, unknown>): Promise<Record<string, unknown>> {
    this.start();
    return new Promise((resolve, reject) => {
      this.waiting.push((line) => {
        try {
          resolve(JSON.parse(line));
        } catch {
          reject(new Error(`엔진 응답이 JSON 이 아니다: ${line.slice(0, 200)}`));
        }
      });
      this.proc!.stdin.write(JSON.stringify(msg) + "\n");
    });
  }

  private async must(msg: Record<string, unknown>) {
    const r = await this.send(msg);
    if (r.ok !== true) throw new Error(`엔진이 ${String(msg.cmd)} 를 거절했다: ${String(r.error)}`);
  }

  open(dbPath: string, tz: string) {
    return this.must({ cmd: "open", db: dbPath, tz });
  }

  async run(op: string, args: unknown[], now: number): Promise<Outcome> {
    const r = await this.send({ cmd: "op", op, args, now });
    return r.ok === true ? { ok: true, result: r.result ?? null } : { ok: false, error: String(r.error) };
  }

  async close() {
    if (!this.proc) return;
    await this.must({ cmd: "close" });
  }

  migrate(dbPath: string) {
    return this.must({ cmd: "migrate", db: dbPath });
  }

  private caps: Promise<{ ops: Set<string> | null; migrate: boolean }> | null = null;

  /** 아는 op 와 migrate 지원. ops 를 모르는 엔진은 전부 안다고 본다 */
  capabilities() {
    this.caps ??= this.send({ cmd: "ops" }).then((r) =>
      r.ok === true && Array.isArray(r.ops)
        ? { ops: new Set(r.ops as string[]), migrate: r.migrate !== false }
        : { ops: null, migrate: true },
    );
    return this.caps;
  }

  /** 픽스처를 다 돌고 프로세스를 내린다 */
  dispose() {
    this.lines?.close();
    this.proc?.stdin.end();
    this.proc = null;
  }
}
