import { describe, expect, it } from "vitest";
import { commonTablePrefix, generateErd, inferReferences } from "./erdGen";
import type {
  SchemaSnapshot,
  SnapshotCheck,
  SnapshotColumn,
  SnapshotForeignKey,
  SnapshotIndex,
  SnapshotTable,
} from "./schemaSnapshot";

// ---- 픽스처 도우미 ----
//
// MySQL information_schema 가 주는 형태를 흉내 낸다. key 는 MySQL 규약대로: PRI / UNI /
// MUL(인덱스의 첫 컬럼) / ''(나머지).

function col(name: string, data_type: string, o: Partial<SnapshotColumn> = {}): SnapshotColumn {
  return {
    name,
    data_type,
    column_type: o.column_type ?? data_type,
    nullable: o.nullable ?? false,
    key: o.key ?? "",
    default_value: o.default_value ?? null,
    extra: o.extra ?? "",
    comment: o.comment ?? "",
  };
}

const ix = (name: string, columns: string[], unique = false): SnapshotIndex => ({
  name,
  unique,
  columns,
});

function tbl(
  name: string,
  cols: SnapshotColumn[],
  o: {
    comment?: string;
    indexes?: SnapshotIndex[];
    fks?: SnapshotForeignKey[];
    checks?: SnapshotCheck[];
  } = {},
): SnapshotTable {
  return {
    name,
    comment: o.comment ?? "",
    rows_estimate: null,
    columns: cols,
    indexes: o.indexes ?? [],
    foreign_keys: o.fks ?? [],
    checks: o.checks ?? [],
  };
}

function snap(tables: SnapshotTable[], schema = "svc_booking"): SchemaSnapshot {
  return {
    amber: 1,
    connection: "01JCONN",
    schema,
    server: "MySQL 8.0.36",
    synced_at: 0,
    fingerprint: "test",
    tables,
  };
}

const pk = (name = "id", data_type = "bigint") =>
  col(name, data_type, { key: "PRI", extra: "auto_increment" });

const FIXTURE: SnapshotTable[] = [
  tbl(
    "ts_booking",
    [
      pk(),
      // 매칭되는 테이블이 없다 → unresolved (선도 마커도 없음, 인덱스만 설명에)
      col("user_id", "bigint", { key: "MUL", comment: "회원 ID" }),
      col("booking_no", "varchar", { column_type: "varchar(32)", key: "UNI", comment: "예약번호" }),
      col("status", "varchar", {
        column_type: "enum('PENDING','CONFIRMED','CANCELED')",
        key: "MUL",
      }),
      col("cancel_reason", "varchar", {
        column_type: "varchar(255)",
        nullable: true,
        comment: "취소 사유",
      }),
      col("created_at", "datetime"),
      col("updated_at", "datetime"),
    ],
    { comment: "예약", indexes: [ix("idx_user_id", ["user_id"]), ix("idx_status", ["status"])] },
  ),
  tbl(
    "ts_booking_passenger",
    [
      pk(),
      col("booking_id", "bigint", { key: "MUL", comment: "예약 ID" }),
      col("name", "varchar", { column_type: "varchar(255)", nullable: true, comment: "이름 (암호화)" }),
      // 코멘트엔 암호화 언급이 없지만 형제 규칙(개인정보 이름 + varchar(255+))으로 (enc)
      col("passport_no", "varchar", { column_type: "varchar(255)", nullable: true, comment: "여권번호" }),
      col("created_at", "datetime"),
    ],
    { comment: "예약 승객", indexes: [ix("ix_booking_id", ["booking_id"])] },
  ),
  tbl(
    "ts_booking_payment",
    [
      pk(),
      col("booking_id", "bigint", { key: "UNI" }),
      col("method", "varchar", { column_type: "varchar(20)" }),
      col("amount", "decimal", { column_type: "decimal(12,2)" }),
      col("created_at", "datetime"),
      col("updated_at", "datetime", { nullable: true }),
    ],
    {
      comment: "결제",
      indexes: [ix("uq_booking_id", ["booking_id"], true)],
      // MySQL 8 은 CHECK 절의 리터럴에 문자셋 접두사를 붙여 저장한다
      checks: [
        {
          name: "chk_payment_method",
          clause: "(`method` in (_utf8mb4'CARD',_utf8mb4'BANK_TRANSFER'))",
        },
      ],
    },
  ),
  tbl(
    "ts_booking_refund",
    [pk(), col("payment_id", "bigint", { key: "MUL", comment: "결제 ID" }), col("created_at", "datetime")],
    {
      comment: "환불",
      indexes: [ix("ix_payment_id", ["payment_id"])],
      fks: [
        {
          name: "fk_refund_payment",
          columns: ["payment_id"],
          ref_schema: "svc_booking",
          ref_table: "ts_booking_payment",
          ref_columns: ["id"],
        },
      ],
    },
  ),
  // 테이블 코멘트 없음 → 라벨 의미는 접두사를 뗀 이름(booking_memo)
  tbl("ts_booking_memo", [
    pk(),
    col("booking_id", "bigint"),
    col("body", "text", { nullable: true, comment: 'a "quoted" memo' }),
    col("created_at", "datetime"),
  ]),
  tbl(
    "ts_booking_coupon_use",
    [
      pk(),
      col("booking_id", "bigint", { key: "MUL" }),
      col("coupon_code", "varchar", { column_type: "varchar(64)" }),
      col("created_at", "datetime"),
    ],
    { comment: "쿠폰 사용", indexes: [ix("uk_booking_coupon", ["booking_id", "coupon_code"], true)] },
  ),
  tbl("revinfo", [pk("rev", "int"), col("revtstmp", "bigint")]),
  tbl("ts_booking_aud", [
    col("id", "bigint", { key: "PRI" }),
    col("rev", "int", { key: "PRI" }),
    col("revtype", "tinyint", { nullable: true }),
    col("status", "varchar", { column_type: "varchar(20)", nullable: true }),
    col("created_at", "datetime", { nullable: true }),
  ]),
];

const HEADER = "    %% amber:db dev/svc_booking · 2026-09-02 09:41";

// 규칙에서 손으로 유도한 기대 출력. 공통 접두사는 'ts_booking' 의 마지막 '_' 까지 물린 'ts_' 라서
// 참조 대상은 booking.id / booking_payment.id, 코멘트 없는 memo 의 라벨은 booking_memo 다.
const GOLDEN_KO = `erDiagram
    %% amber:db dev/svc_booking · 2026-09-02 09:41
    %% 실선(--)=물리 FK(DB 제약) / 점선(..)=논리 참조(앱 레벨, 물리 FK 없음)
    ts_booking ||..o{ ts_booking_coupon_use : "쿠폰 사용 · 논리 FK(uk_booking_coupon)"
    ts_booking ||..o{ ts_booking_memo : "booking_memo · 논리 FK, DB 제약 없음"
    ts_booking ||..o{ ts_booking_passenger : "예약 승객 · 논리 FK(ix_booking_id)"
    ts_booking ||..o| ts_booking_payment : "결제 · uq, 논리"
    ts_booking_payment ||--o{ ts_booking_refund : "환불 · 물리 FK"

    revinfo ||..o{ ts_booking_aud : "Envers rev"

    ts_booking {
        bigint id PK
        bigint user_id "회원 ID (idx_user_id)"
        varchar booking_no UK "예약번호"
        varchar status "PENDING/CONFIRMED/CANCELED (idx_status)"
        varchar? cancel_reason "취소 사유"
        datetime created_at
        datetime updated_at
    }

    ts_booking_coupon_use {
        bigint id PK
        bigint booking_id FK "uk(booking_id,coupon_code); 논리 FK -> booking.id"
        varchar coupon_code
        datetime created_at
    }

    ts_booking_memo {
        bigint id PK
        bigint booking_id FK "논리 FK -> booking.id; DB 제약/인덱스 없음"
        text? body "a #quot;quoted#quot; memo"
        datetime created_at
    }

    ts_booking_passenger {
        bigint id PK
        bigint booking_id FK "예약 ID (ix_booking_id); 논리 FK -> booking.id"
        varchar? name "이름 (암호화) (enc)"
        varchar? passport_no "여권번호 (enc)"
        datetime created_at
    }

    ts_booking_payment {
        bigint id PK
        bigint booking_id FK,UK "논리 FK -> booking.id"
        varchar method "CARD/BANK_TRANSFER"
        decimal amount
        datetime created_at
        datetime? updated_at
    }

    ts_booking_refund {
        bigint id PK
        bigint payment_id FK "결제 ID (ix_payment_id); 물리 FK -> booking_payment.id"
        datetime created_at
    }

    revinfo {
        int rev PK
        bigint revtstmp
    }

    ts_booking_aud {
        bigint id PK
        int rev PK,FK
        tinyint revtype
        varchar status
    }
`;

describe("generateErd — 골든", () => {
  it("픽스처 전체를 하우스 스타일 그대로 낸다 (ko)", () => {
    const r = generateErd(snap(FIXTURE), { lang: "ko", header: HEADER });
    expect(r.mermaid).toBe(GOLDEN_KO);
    expect(r.stats).toEqual({
      tables: 8,
      auditTables: 1,
      physicalFk: 1,
      logicalRefs: 4,
      unresolvedRefs: 1,
    });
    expect(r.unresolved).toEqual([{ table: "ts_booking", column: "user_id" }]);
  });

  it("헤더가 없으면 erDiagram 바로 다음이 범례다", () => {
    const r = generateErd(snap(FIXTURE), { lang: "ko" });
    expect(r.mermaid.split("\n").slice(0, 2)).toEqual([
      "erDiagram",
      "    %% 실선(--)=물리 FK(DB 제약) / 점선(..)=논리 참조(앱 레벨, 물리 FK 없음)",
    ]);
    expect(r.mermaid).toBe(GOLDEN_KO.replace(`${HEADER}\n`, ""));
  });

  it("테이블 순서를 섞어도 바이트 단위로 같은 출력이다", () => {
    const expected = generateErd(snap(FIXTURE), { lang: "ko", header: HEADER }).mermaid;
    const reversed = [...FIXTURE].reverse();
    const rotated = [...FIXTURE.slice(3), ...FIXTURE.slice(0, 3)];
    const interleaved = FIXTURE.filter((_, i) => i % 2).concat(FIXTURE.filter((_, i) => !(i % 2)));
    for (const tables of [reversed, rotated, interleaved]) {
      expect(generateErd(snap(tables), { lang: "ko", header: HEADER }).mermaid).toBe(expected);
    }
  });
});

describe("generateErd — 언어", () => {
  it("en 은 범례·근거 단어·참조 문구만 바뀌고 코멘트는 그대로다", () => {
    const { mermaid } = generateErd(snap(FIXTURE), { lang: "en" });
    const lines = mermaid.split("\n");
    expect(lines[1]).toBe(
      "    %% solid(--)=physical FK (DB constraint) / dotted(..)=logical reference (app level, no FK constraint)",
    );
    expect(lines).toContain('    ts_booking_payment ||--o{ ts_booking_refund : "환불 · physical FK"');
    expect(lines).toContain('    ts_booking ||..o{ ts_booking_passenger : "예약 승객 · logical FK(ix_booking_id)"');
    expect(lines).toContain('    ts_booking ||..o| ts_booking_payment : "결제 · uq, logical"');
    expect(lines).toContain('    ts_booking ||..o{ ts_booking_memo : "booking_memo · logical FK, no DB constraint"');
    expect(lines).toContain('        bigint booking_id FK "logical FK -> booking.id; no DB constraint or index"');
    expect(lines).toContain('        bigint payment_id FK "결제 ID (ix_payment_id); physical FK -> booking_payment.id"');
    expect(mermaid).not.toMatch(/물리|논리|제약/);
  });
});

describe("generateErd — 부분집합", () => {
  it("ts_booking 만 고르면 짝 감사 테이블과 revinfo 가 딸려오고 도메인 선은 없다", () => {
    const r = generateErd(snap(FIXTURE), { lang: "ko", tables: ["ts_booking"] });
    expect(r.mermaid).toBe(`erDiagram
    %% 실선(--)=물리 FK(DB 제약) / 점선(..)=논리 참조(앱 레벨, 물리 FK 없음)

    revinfo ||..o{ ts_booking_aud : "Envers rev"

    ts_booking {
        bigint id PK
        bigint user_id "회원 ID (idx_user_id)"
        varchar booking_no UK "예약번호"
        varchar status "PENDING/CONFIRMED/CANCELED (idx_status)"
        varchar? cancel_reason "취소 사유"
        datetime created_at
        datetime updated_at
    }

    revinfo {
        int rev PK
        bigint revtstmp
    }

    ts_booking_aud {
        bigint id PK
        int rev PK,FK
        tinyint revtype
        varchar status
    }
`);
    // unresolved 는 부분집합이 아니라 스냅샷 전체 기준
    expect(r.stats).toEqual({ tables: 3, auditTables: 1, physicalFk: 0, logicalRefs: 0, unresolvedRefs: 1 });
  });

  it("상대가 그려지지 않은 관계는 선·FK 마커 없이 설명에만 남는다", () => {
    const { mermaid, stats } = generateErd(snap(FIXTURE), {
      lang: "ko",
      tables: ["ts_booking_refund", "ts_booking_passenger"],
    });
    expect(mermaid).not.toMatch(/\|\|[-.]{2}o[{|]/);
    expect(mermaid).toContain('        bigint payment_id "결제 ID (ix_payment_id); 물리 FK -> booking_payment.id"');
    expect(mermaid).toContain('        bigint booking_id "예약 ID (ix_booking_id); 논리 FK -> booking.id"');
    // 감사 테이블이 없으니 revinfo 도 안 딸려온다
    expect(mermaid).not.toContain("revinfo");
    expect(stats).toEqual({ tables: 2, auditTables: 0, physicalFk: 0, logicalRefs: 0, unresolvedRefs: 1 });
    // 관계 없는 도메인 테이블은 이름순
    expect(mermaid.indexOf("ts_booking_passenger {")).toBeLessThan(mermaid.indexOf("ts_booking_refund {"));
  });

  it("없는 테이블 이름은 무시한다", () => {
    const r = generateErd(snap(FIXTURE), { lang: "ko", tables: ["nope", "revinfo"] });
    expect(r.stats.tables).toBe(1);
    expect(r.mermaid).toContain("    revinfo {");
  });
});

describe("참조 추론 보강 — 실제 accounts 스키마에서 나온 구멍들", () => {
  const customers = tbl("ts_customers", [pk(), col("name", "varchar", { column_type: "varchar(50)" })], { comment: "회원" });
  const devices = tbl("ts_devices", [pk(), col("device_key", "varchar", { column_type: "varchar(64)", key: "UNI" })], {
    comment: "디바이스",
  });

  it("코멘트의 `table.column` 이 이름 규칙으로 못 잇는 user_id → ts_customers 를 잇고, 설명은 참조를 되풀이하지 않는다", () => {
    const consents = tbl(
      "ts_customer_consents",
      [pk(), col("user_id", "bigint", { nullable: true, comment: "ts_customers.id - 유저 ID" })],
      { comment: "수신동의" },
    );
    const r = generateErd(snap([customers, consents]), { lang: "ko" });
    expect(r.mermaid).toContain('ts_customers ||..o{ ts_customer_consents : "수신동의 · 논리 FK(코멘트)"');
    expect(r.mermaid).toContain('bigint? user_id FK "ts_customers.id - 유저 ID; DB 제약/인덱스 없음"');
    expect(r.stats.unresolvedRefs).toBe(0);
  });

  it("공통 접두사가 없어도 대다수 토큰(ts_)을 접두사로 삼아 device_id → ts_devices 를 찾는다", () => {
    const consents = tbl("ts_customer_consents", [pk(), col("device_id", "bigint", { key: "MUL" })], {
      indexes: [ix("ix_device", ["device_id"])],
    });
    const logs = tbl("editing_logs", [pk(), col("target_id", "bigint")]);
    const i18n = tbl("i18n_messages", [pk("no", "int"), col("code", "varchar")]);
    expect(commonTablePrefix([customers.name, devices.name, consents.name, logs.name, i18n.name])).toBe("ts_");
    const { edges } = inferReferences(snap([customers, devices, consents, logs, i18n]));
    expect(edges.map((e) => `${e.from}.${e.fromColumn} -> ${e.to}.${e.toColumn}`)).toEqual([
      "ts_customer_consents.device_id -> ts_devices.id",
    ]);
  });

  it("`_x` 로 끝나는 테이블이 하나면 잇고(profile_id → ts_user_profiles), 둘이면 추측하지 않는다(share_id)", () => {
    const profiles = tbl("ts_user_profiles", [pk()]);
    const members = tbl("ts_org_members", [pk(), col("profile_id", "bigint")]);
    const shareA = tbl("ts_travel_like_share", [pk()]);
    const shareB = tbl("ts_domestic_property_like_share", [pk()]);
    const item = tbl("ts_travel_like_share_item", [pk(), col("share_id", "bigint")]);
    const { edges, unresolved } = inferReferences(snap([profiles, members, shareA, shareB, item]));
    expect(edges.map((e) => `${e.from}.${e.fromColumn} -> ${e.to}`)).toEqual([
      "ts_org_members.profile_id -> ts_user_profiles",
    ]);
    expect(unresolved).toEqual([{ table: "ts_travel_like_share_item", column: "share_id" }]);
  });

  it("타입이 안 맞으면 id 대신 같은 이름의 UNIQUE 컬럼을 가리킨다 (varchar travel_id → ts_travels.travel_id)", () => {
    const travels = tbl("ts_travels", [
      pk(),
      col("travel_id", "varchar", { column_type: "varchar(36)", key: "UNI" }),
    ]);
    const dibs = tbl("ts_user_travel_interactions", [
      col("travel_id", "varchar", { column_type: "varchar(36)", key: "PRI" }),
      col("user_id", "bigint", { key: "PRI" }),
      col("trip_code", "varchar"),
    ]);
    // PK 컬럼은 추론 대상이 아니다 — travel_id 가 일반 컬럼인 테이블로 본다
    const queue = tbl("ts_travel_change_queue", [pk(), col("travel_id", "varchar", { column_type: "varchar(36)" })]);
    const { edges } = inferReferences(snap([travels, dibs, queue]));
    expect(edges.map((e) => `${e.from}.${e.fromColumn} -> ${e.to}.${e.toColumn}`)).toEqual([
      "ts_travel_change_queue.travel_id -> ts_travels.travel_id",
    ]);
  });

  it("같은 두 테이블 사이의 FK 가 셋이면 관계선은 하나에 ×3, 관계가 많은 부모가 먼저 온다", () => {
    const contacts = tbl("ts_personal_contacts", [pk()], { comment: "연락처" });
    const orgs = tbl("ts_organizations", [pk()], { comment: "조직" });
    const fkTo = (name: string, column: string, ref_table: string) => ({
      name,
      columns: [column],
      ref_schema: "svc_booking",
      ref_table,
      ref_columns: ["id"],
    });
    const details = tbl(
      "ts_org_details",
      [
        pk(),
        col("org_id", "bigint", { comment: "조직 ID. FK to ts_organizations.id" }),
        col("contract_manager_contact_id", "bigint"),
        col("primary_settlement_contact_id", "bigint"),
        col("secondary_settlement_contact_id", "bigint", { nullable: true }),
      ],
      {
        comment: "조직 상세",
        indexes: [ix("org_id", ["org_id"]), ix("contract_manager_contact_id", ["contract_manager_contact_id"])],
        fks: [
          fkTo("org_id", "org_id", "ts_organizations"),
          fkTo("contract_manager_contact_id", "contract_manager_contact_id", "ts_personal_contacts"),
          fkTo("primary_settlement_contact_id", "primary_settlement_contact_id", "ts_personal_contacts"),
          fkTo("secondary_settlement_contact_id", "secondary_settlement_contact_id", "ts_personal_contacts"),
        ],
      },
    );
    const members = tbl("ts_org_members", [pk(), col("org_id", "bigint")], {
      comment: "구성원",
      fks: [fkTo("fk_members_org", "org_id", "ts_organizations")],
    });
    const r = generateErd(snap([contacts, orgs, details, members]), { lang: "ko" });
    const lines = r.mermaid.split("\n").filter((l) => l.includes("||--"));
    expect(lines).toEqual([
      '    ts_organizations ||--o{ ts_org_details : "조직 상세 · 물리 FK"',
      '    ts_organizations ||--o{ ts_org_members : "구성원 · 물리 FK"',
      '    ts_personal_contacts ||--o{ ts_org_details : "조직 상세 · 물리 FK ×3"',
    ]);
    // FK 자동 인덱스(제약과 같은 이름)는 적지 않고, 코멘트가 대상을 말하면 참조 문구도 되풀이하지 않는다
    expect(r.mermaid).toContain('bigint org_id FK "조직 ID. FK to ts_organizations.id"');
    expect(r.mermaid).toContain('bigint contract_manager_contact_id FK "물리 FK -> personal_contacts.id"');
    expect(r.stats.physicalFk).toBe(5);
  });

  it("id PK 의 코멘트가 'ID' 같은 되풀이면 비우고, 깨진 인코딩 코멘트는 되살린다", () => {
    // 실제 장애 모양대로: UTF-8 바이트를 windows-1252 로 읽어 저장한 코멘트
    const broken = new TextDecoder("windows-1252").decode(new TextEncoder().encode("마지막 변경 감지 시각"));
    expect(broken).not.toBe("마지막 변경 감지 시각");
    const t = tbl("ts_queue", [pk("id", "bigint"), col("changed_at", "datetime", { comment: broken })]);
    t.columns[0].comment = "ID";
    const r = generateErd(snap([t]), { lang: "ko" });
    expect(r.mermaid).toContain("        bigint id PK\n");
    expect(r.mermaid).toContain('datetime changed_at "마지막 변경 감지 시각"');
  });
});

describe("generateErd — 감사 테이블 제외", () => {
  it("audit: false 면 *_aud·revinfo 블록과 Envers 관계선이 사라지고 도메인만 남는다", () => {
    const r = generateErd(snap(FIXTURE), { lang: "ko", audit: false });
    expect(r.mermaid).not.toContain("_aud");
    expect(r.mermaid).not.toContain("revinfo");
    expect(r.mermaid).not.toContain("Envers");
    expect(r.stats.auditTables).toBe(0);
    expect(r.stats.tables).toBe(FIXTURE.filter((t) => !t.name.endsWith("_aud") && t.name !== "revinfo").length);
    // 도메인 관계선과 블록은 그대로
    expect(r.mermaid).toContain('ts_booking_payment ||--o{ ts_booking_refund : "환불 · 물리 FK"');
    expect(r.mermaid).toContain("    ts_booking {");
  });

  it("부분집합과 함께 써도 감사 테이블은 딸려오지 않는다", () => {
    const r = generateErd(snap(FIXTURE), { lang: "ko", tables: ["ts_booking"], audit: false });
    expect(r.mermaid).not.toContain("ts_booking_aud");
    expect(r.stats).toEqual({ tables: 1, auditTables: 0, physicalFk: 0, logicalRefs: 0, unresolvedRefs: 1 });
  });
});

describe("inferReferences", () => {
  it("픽스처에서 물리 1 + 논리 4, user_id 는 unresolved", () => {
    const { edges, unresolved } = inferReferences(snap(FIXTURE));
    expect(edges.filter((e) => e.physical)).toHaveLength(1);
    expect(edges.filter((e) => !e.physical)).toHaveLength(4);
    expect(unresolved).toEqual([{ table: "ts_booking", column: "user_id" }]);
    // (to, from, fromColumn) 순
    expect(edges.map((e) => `${e.to} <- ${e.from}.${e.fromColumn}`)).toEqual([
      "ts_booking <- ts_booking_coupon_use.booking_id",
      "ts_booking <- ts_booking_memo.booking_id",
      "ts_booking <- ts_booking_passenger.booking_id",
      "ts_booking <- ts_booking_payment.booking_id",
      "ts_booking_payment <- ts_booking_refund.payment_id",
    ]);
  });

  it("엣지 세부 — 1:1·인덱스 이름·제약 이름", () => {
    const { edges } = inferReferences(snap(FIXTURE));
    const byFrom = new Map(edges.map((e) => [e.from, e]));
    expect(byFrom.get("ts_booking_payment")).toMatchObject({
      physical: false,
      oneToOne: true,
      indexName: "uq_booking_id",
      toColumn: "id",
      constraintName: null,
    });
    // 복합 UNIQUE 의 첫 컬럼: 인덱스 이름은 잡히지만 1:1 은 아니다
    expect(byFrom.get("ts_booking_coupon_use")).toMatchObject({
      oneToOne: false,
      indexName: "uk_booking_coupon",
    });
    expect(byFrom.get("ts_booking_memo")).toMatchObject({ oneToOne: false, indexName: null });
    expect(byFrom.get("ts_booking_refund")).toMatchObject({
      physical: true,
      oneToOne: false,
      indexName: "ix_payment_id",
      constraintName: "fk_refund_payment",
      to: "ts_booking_payment",
      toColumn: "id",
    });
  });

  it("후보가 둘이면 추측하지 않고 unresolved 로 남긴다", () => {
    const tables = [
      tbl("ts_order", [pk()]),
      tbl("ts_orders", [pk()]),
      tbl("ts_order_item", [pk(), col("order_id", "bigint")]),
    ];
    const { edges, unresolved } = inferReferences(snap(tables));
    expect(edges).toEqual([]);
    expect(unresolved).toEqual([{ table: "ts_order_item", column: "order_id" }]);
  });

  it("복수형·y→ies·자기 참조 제외·id PK 없는 테이블 제외", () => {
    const tables = [
      tbl("ts_categories", [pk()]),
      tbl("ts_users", [pk()]),
      // id 가 PK 가 아니라 후보에서 빠진다
      tbl("ts_tags", [col("id", "bigint"), col("code", "varchar", { key: "PRI" })]),
      tbl("ts_posts", [
        pk(),
        col("category_id", "bigint"),
        col("user_id", "bigint"),
        col("tag_id", "bigint"),
        col("post_id", "bigint"),
      ]),
    ];
    const { edges, unresolved } = inferReferences(snap(tables));
    expect(edges.map((e) => `${e.fromColumn}->${e.to}`)).toEqual([
      "category_id->ts_categories",
      "user_id->ts_users",
    ]);
    expect(unresolved).toEqual([
      { table: "ts_posts", column: "post_id" },
      { table: "ts_posts", column: "tag_id" },
    ]);
  });

  it("감사 테이블과 revinfo 는 논리 추론에 끼지 않는다", () => {
    const tables = [
      tbl("ts_booking", [pk()]),
      tbl("revinfo", [pk("rev", "int"), col("booking_id", "bigint")]),
      tbl("ts_booking_aud", [col("id", "bigint", { key: "PRI" }), col("rev", "int", { key: "PRI" }), col("booking_id", "bigint")]),
      // *_id 가 감사 테이블을 가리켜도 잇지 않는다
      tbl("ts_note", [pk(), col("booking_aud_id", "bigint")]),
    ];
    const { edges, unresolved } = inferReferences(snap(tables));
    expect(edges).toEqual([]);
    expect(unresolved).toEqual([{ table: "ts_note", column: "booking_aud_id" }]);
  });
});

describe("commonTablePrefix", () => {
  it("공통 문자열을 마지막 '_' 까지 물린다", () => {
    // 'ts_booking' 은 단어 경계가 아니므로 'ts_' 까지
    expect(commonTablePrefix(["ts_booking", "ts_booking_passenger", "ts_booking_payment"])).toBe("ts_");
    expect(commonTablePrefix(["ts_identity_esigns", "ts_identity_esign_signers"])).toBe("ts_identity_");
    expect(commonTablePrefix(["ts_a", "ts_b"])).toBe("ts_");
  });

  it("이름이 둘 미만이거나 '_' 없는 공통 부분은 ''", () => {
    expect(commonTablePrefix(["a"])).toBe("");
    expect(commonTablePrefix([])).toBe("");
    expect(commonTablePrefix(["abc", "abd"])).toBe("");
    expect(commonTablePrefix(["ts_x", "user"])).toBe("");
  });

  it("라벨 의미의 기본값과 참조 대상이 같은 접두사를 쓴다", () => {
    const tables = [
      tbl("ts_identity_esigns", [pk()]),
      tbl("ts_identity_esign_signers", [pk(), col("esign_id", "bigint", { key: "MUL" })], {
        indexes: [ix("ix_esign_id", ["esign_id"])],
      }),
    ];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain('    ts_identity_esigns ||..o{ ts_identity_esign_signers : "esign_signers · 논리 FK(ix_esign_id)"');
    expect(mermaid).toContain('        bigint esign_id FK "(ix_esign_id); 논리 FK -> esigns.id"');
  });
});

describe("속성 줄 세부 규칙", () => {
  it("created_at → updated_at 은 항상 맨 뒤로 가고 설명을 달지 않는다", () => {
    const tables = [
      tbl("t_thing", [
        col("updated_at", "datetime", { nullable: true, comment: "수정 시각" }),
        col("created_at", "datetime", { key: "MUL", comment: "생성 시각" }),
        pk(),
        col("label", "varchar", { column_type: "varchar(50)", comment: "라벨" }),
      ], { indexes: [ix("idx_created_at", ["created_at"])] }),
    ];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain(`    t_thing {
        bigint id PK
        varchar label "라벨"
        datetime created_at
        datetime? updated_at
    }
`);
  });

  it("따옴표는 라벨·설명 모두 #quot; 로 바꾼다", () => {
    const tables = [
      tbl("ts_a", [pk()], { comment: "A" }),
      tbl("ts_b", [pk(), col("a_id", "bigint", { comment: 'say "hi"' })], { comment: 'the "b" table' }),
    ];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain('    ts_a ||..o{ ts_b : "the #quot;b#quot; table · 논리 FK, DB 제약 없음"');
    expect(mermaid).toContain('        bigint a_id FK "say #quot;hi#quot;; 논리 FK -> a.id; DB 제약/인덱스 없음"');
    expect(mermaid.split("\n").filter((l) => l.includes('"')).every((l) => l.split('"').length === 3)).toBe(true);
  });

  it("PK 는 스냅샷이 nullable 이라 해도 ? 를 달지 않는다", () => {
    const tables = [tbl("t", [col("id", "bigint", { key: "PRI", nullable: true }), col("x", "int", { nullable: true })])];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain("        bigint id PK\n        int? x\n");
  });

  it("코멘트가 없으면 이름에서 의미를 지어내지 않는다", () => {
    const tables = [tbl("t", [pk(), col("customer_name", "varchar", { column_type: "varchar(100)" })])];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain("        varchar customer_name\n");
  });

  it("(enc) — 코멘트 근거가 있을 때만 형제 규칙이 켜지고, 좁은 컬럼은 제외", () => {
    const withEnc = tbl("t", [
      pk(),
      col("email", "varchar", { column_type: "varchar(512)", comment: "encrypted email" }),
      col("phone", "text"),
      col("nickname", "varchar", { column_type: "varchar(50)" }),
      col("address", "varchar", { column_type: "varchar(254)" }),
      col("memo", "text"),
    ]);
    const enc = generateErd(snap([withEnc]), { lang: "ko" }).mermaid;
    expect(enc).toContain('        varchar email "encrypted email (enc)"');
    expect(enc).toContain('        text phone "(enc)"');
    expect(enc).toContain("        varchar nickname\n");
    expect(enc).toContain("        varchar address\n");
    expect(enc).toContain("        text memo\n");

    const noEnc = tbl("t", [pk(), col("phone", "text"), col("passport_no", "varchar", { column_type: "varchar(255)" })]);
    const plain = generateErd(snap([noEnc]), { lang: "ko" }).mermaid;
    expect(plain).not.toContain("(enc)");
  });

  it("enum — column_type 이 CHECK 보다 우선하고, 둘 다 없으면 쓰지 않는다", () => {
    const tables = [
      tbl(
        "t",
        [
          pk(),
          col("kind", "enum", { column_type: "enum('A','B')", comment: "종류" }),
          col("state", "varchar", { column_type: "varchar(10)" }),
          col("substate", "varchar", { column_type: "varchar(10)" }),
          col("mode", "varchar", { column_type: "varchar(10)" }),
        ],
        {
          checks: [
            { name: "c1", clause: "(`kind` in ('X','Y'))" },
            { name: "c2", clause: "(`substate` in ('S1','S2'))" },
            { name: "c3", clause: "(state IN ('ON', 'OFF'))" },
          ],
        },
      ),
    ];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain('        enum kind "종류; A/B"');
    expect(mermaid).toContain('        varchar state "ON/OFF"');
    expect(mermaid).toContain('        varchar substate "S1/S2"');
    expect(mermaid).toContain("        varchar mode\n");
  });

  it("다른 스키마를 가리키는 물리 FK — 사실은 적되 마커·선은 없고 논리 추론에서도 빠진다", () => {
    const tables = [
      tbl("ts_user", [pk()]),
      tbl(
        "ts_order",
        [pk(), col("user_id", "bigint", { key: "MUL" })],
        {
          indexes: [ix("ix_user_id", ["user_id"])],
          fks: [
            {
              name: "fk_order_user",
              columns: ["user_id"],
              ref_schema: "svc_identity",
              ref_table: "ts_identity_users",
              ref_columns: ["id"],
            },
          ],
        },
      ),
    ];
    const r = generateErd(snap(tables), { lang: "ko" });
    expect(r.mermaid).not.toMatch(/\|\|[-.]{2}o[{|]/);
    expect(r.mermaid).toContain('        bigint user_id "(ix_user_id); 물리 FK -> svc_identity.ts_identity_users.id"');
    expect(r.unresolved).toEqual([]);
    expect(r.stats.physicalFk).toBe(0);
  });

  it("물리 FK 1:1 은 실선 o| 와 'uq, 물리 FK'", () => {
    const tables = [
      tbl("ts_a", [pk()], { comment: "A" }),
      tbl("ts_b", [pk(), col("a_id", "bigint", { key: "UNI" })], {
        comment: "B",
        indexes: [ix("uq_a_id", ["a_id"], true)],
        fks: [{ name: "fk_b_a", columns: ["a_id"], ref_schema: "svc_booking", ref_table: "ts_a", ref_columns: ["id"] }],
      }),
    ];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain('    ts_a ||--o| ts_b : "B · uq, 물리 FK"');
    expect(mermaid).toContain('        bigint a_id FK,UK "물리 FK -> a.id"');
  });

  it("이름의 이상한 문자는 _ 로, 타입은 소문자 한 토큰으로", () => {
    const tables = [tbl("odd table", [pk(), col("my col", "Double Precision", { nullable: true })])];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain("    odd_table {\n        bigint id PK\n        double_precision? my_col\n    }\n");
  });
});

describe("감사 테이블", () => {
  it("id/rev/revtype 을 머리로 올리고 나머지는 스냅샷 순서, 타임스탬프·?·설명 없음", () => {
    const tables = [
      tbl("ts_x", [pk()]),
      tbl("revinfo", [pk("rev", "int"), col("revtstmp", "bigint", { nullable: true })]),
      tbl("ts_x_aud", [
        col("status", "varchar", { column_type: "varchar(20)", nullable: true, comment: "상태" }),
        col("rev", "int", { key: "PRI" }),
        col("updated_at", "datetime", { nullable: true }),
        col("revtype", "tinyint", { nullable: true }),
        col("amount", "decimal", { nullable: true }),
        col("id", "bigint", { key: "PRI" }),
        col("created_at", "datetime", { nullable: true }),
      ]),
    ];
    const { mermaid, stats } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toContain(`    ts_x_aud {
        bigint id PK
        int rev PK,FK
        tinyint revtype
        varchar status
        decimal amount
    }
`);
    // revinfo 는 일반 스타일 — 널 허용이면 ? 가 붙는다
    expect(mermaid).toContain("    revinfo {\n        int rev PK\n        bigint? revtstmp\n    }");
    expect(stats).toEqual({ tables: 3, auditTables: 1, physicalFk: 0, logicalRefs: 0, unresolvedRefs: 0 });
  });

  it("revinfo 가 없으면 감사 관계선도 없다", () => {
    const tables = [tbl("ts_x", [pk()]), tbl("ts_x_aud", [col("id", "bigint", { key: "PRI" }), col("rev", "int", { key: "PRI" })])];
    const { mermaid } = generateErd(snap(tables), { lang: "ko" });
    expect(mermaid).toBe(`erDiagram
    %% 실선(--)=물리 FK(DB 제약) / 점선(..)=논리 참조(앱 레벨, 물리 FK 없음)

    ts_x {
        bigint id PK
    }

    ts_x_aud {
        bigint id PK
        int rev PK,FK
    }
`);
  });
});
