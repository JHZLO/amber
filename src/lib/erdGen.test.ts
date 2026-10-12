import { describe, expect, it } from "vitest";
import { compile, format } from "resin";
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

/** 생성하고, resin 이 오류 없이 읽는지까지 본다 */
const gen = (tables: SnapshotTable[], opts?: { audit?: boolean }): string => {
  const { source } = generateErd(snap(tables), opts);
  expect(compile(source).diagnostics.filter((d) => d.severity === "error")).toEqual([]);
  return source;
};
/** 컬럼 한 줄 — 정렬 공백은 두 칸으로 접는다 */
const line = (source: string, column: string): string =>
  source
    .split("\n")
    .find((l) => l.trim().split(/\s+/)[0] === column)!
    .trim()
    .replace(/\s{2,}/g, "  ");

// 규칙에서 유도한 기대 출력. 선언된 FK 는 `->`, 이름이나 코멘트로 추론한 참조는 `~>`, 감사 테이블은 원본의
// `audit envers` 한 줄이 된다. user_id 는 가리킬 테이블이 없어 인덱스만 남는다.
const GOLDEN = `table ts_booking "예약" {
  id             bigint         pk
  user_id        bigint         index as idx_user_id  "회원 ID"
  booking_no     varchar(32)    uk  "예약번호"
  status         enum           enum(PENDING, CONFIRMED, CANCELED)  index as idx_status
  cancel_reason  varchar(255)?  "취소 사유"
  created_at     datetime
  updated_at     datetime
} audit envers(status, created_at)

table ts_booking_coupon_use "쿠폰 사용" {
  id           bigint       pk
  booking_id   bigint       ~> ts_booking
  coupon_code  varchar(64)
  created_at   datetime
  unique(booking_id, coupon_code) as uk_booking_coupon
}

table ts_booking_memo {
  id          bigint    pk
  booking_id  bigint    ~> ts_booking
  body        text?     "a \\"quoted\\" memo"
  created_at  datetime
}

table ts_booking_passenger "예약 승객" {
  id           bigint         pk
  booking_id   bigint         ~> ts_booking  index as ix_booking_id  "예약 ID"
  name         varchar(255)?  enc  "이름 (암호화)"
  passport_no  varchar(255)?  enc  "여권번호"
  created_at   datetime
}

table ts_booking_payment "결제" {
  id          bigint         pk
  booking_id  bigint         uk as uq_booking_id  ~> ts_booking
  method      varchar(20)    enum(CARD, BANK_TRANSFER)
  amount      decimal(12,2)
  created_at  datetime
  updated_at  datetime?
}

table ts_booking_refund "환불" {
  id          bigint    pk
  payment_id  bigint    -> ts_booking_payment  index as ix_payment_id  "결제 ID"
  created_at  datetime
}
`;

describe("generateErd — 골든", () => {
  it("픽스처 전체를 resin 으로 낸다", () => {
    const r = generateErd(snap(FIXTURE));
    expect(r.source).toBe(GOLDEN);
    expect(r.notes).toEqual([]);
  });

  it("resin 이 경고 없이 읽고, resin 의 정렬과 같은 모양이다", () => {
    const { source } = generateErd(snap(FIXTURE));
    expect(compile(source).diagnostics).toEqual([]);
    expect(format(source).text).toBe(source);
  });

  it("테이블 순서를 섞어도 바이트 단위로 같은 출력이다", () => {
    const expected = generateErd(snap(FIXTURE)).source;
    for (let i = 1; i < FIXTURE.length; i++) {
      const tables = [...FIXTURE.slice(i), ...FIXTURE.slice(0, i)];
      expect(generateErd(snap(tables)).source).toBe(expected);
    }
  });

  it("audit: false 면 감사 테이블을 싣지 않는다", () => {
    const source = gen(FIXTURE, { audit: false });
    expect(source).not.toContain("audit envers");
    expect(source).toBe(GOLDEN.replace("} audit envers(status, created_at)", "}"));
  });
});

describe("참조 추론 보강 — 실제 accounts 스키마에서 나온 구멍들", () => {
  const customers = tbl("ts_customers", [pk(), col("name", "varchar", { column_type: "varchar(50)" })], { comment: "회원" });
  const devices = tbl("ts_devices", [pk(), col("device_key", "varchar", { column_type: "varchar(64)", key: "UNI" })], {
    comment: "디바이스",
  });

  it("코멘트의 `table.column` 이 이름 규칙으로 못 잇는 user_id → ts_customers 를 잇는다", () => {
    const consents = tbl(
      "ts_customer_consents",
      [pk(), col("user_id", "bigint", { nullable: true, comment: "ts_customers.id - 유저 ID" })],
      { comment: "수신동의" },
    );
    expect(line(gen([customers, consents]), "user_id")).toBe('user_id  bigint?  ~> ts_customers  "ts_customers.id - 유저 ID"');
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
    // 테이블은 이름순이라 ts_travel_change_queue 가 먼저 나온다. 대상 컬럼이 기본 키가 아니라 이름까지 적는다
    expect(line(gen([travels, dibs, queue]), "travel_id")).toBe("travel_id  varchar(36)  ~> ts_travels.travel_id");
  });

  it("같은 두 테이블 사이의 FK 가 여럿이면 컬럼마다 하나씩 잇는다", () => {
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
    const source = gen([contacts, orgs, details]);
    expect(line(source, "org_id")).toBe('org_id  bigint  -> ts_organizations  index as org_id  "조직 ID. FK to ts_organizations.id"');
    expect(line(source, "secondary_settlement_contact_id")).toBe("secondary_settlement_contact_id  bigint?  -> ts_personal_contacts");
    expect(compile(source).model!.relations.filter((r) => r.parent === "ts_personal_contacts")).toHaveLength(3);
  });

  it("id PK 의 코멘트가 'ID' 같은 되풀이면 비우고, 깨진 인코딩 코멘트는 되살린다", () => {
    // 실제 장애 모양대로: UTF-8 바이트를 windows-1252 로 읽어 저장한 코멘트
    const broken = new TextDecoder("windows-1252").decode(new TextEncoder().encode("마지막 변경 감지 시각"));
    expect(broken).not.toBe("마지막 변경 감지 시각");
    const t = tbl("ts_queue", [pk("id", "bigint"), col("changed_at", "datetime", { comment: broken })]);
    t.columns[0].comment = "ID";
    const source = gen([t]);
    expect(line(source, "id")).toBe("id  bigint  pk");
    expect(line(source, "changed_at")).toBe('changed_at  datetime  "마지막 변경 감지 시각"');
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

  it("엣지 세부 — 물리 FK 는 선언된 대상 컬럼, 논리 참조는 id", () => {
    const { edges } = inferReferences(snap(FIXTURE));
    const byFrom = new Map(edges.map((e) => [e.from, e]));
    expect(byFrom.get("ts_booking_payment")).toEqual({
      from: "ts_booking_payment",
      fromColumn: "booking_id",
      to: "ts_booking",
      toColumn: "id",
      physical: false,
    });
    expect(byFrom.get("ts_booking_refund")).toEqual({
      from: "ts_booking_refund",
      fromColumn: "payment_id",
      to: "ts_booking_payment",
      toColumn: "id",
      physical: true,
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

  it("접두사가 긴 스키마에서도 참조 대상을 찾는다", () => {
    const tables = [
      tbl("ts_identity_esigns", [pk()]),
      tbl("ts_identity_esign_signers", [pk(), col("esign_id", "bigint", { key: "MUL" })], {
        indexes: [ix("ix_esign_id", ["esign_id"])],
      }),
    ];
    expect(line(gen(tables), "esign_id")).toBe("esign_id  bigint  ~> ts_identity_esigns  index as ix_esign_id");
  });
});

describe("컬럼 세부 규칙", () => {
  it("created_at → updated_at 은 항상 맨 뒤로 간다", () => {
    const tables = [
      tbl("t_thing", [
        col("updated_at", "datetime", { nullable: true, comment: "수정 시각" }),
        col("created_at", "datetime", { key: "MUL", comment: "생성 시각" }),
        pk(),
        col("label", "varchar", { column_type: "varchar(50)", comment: "라벨" }),
      ], { indexes: [ix("idx_created_at", ["created_at"])] }),
    ];
    const names = gen(tables).split("\n").slice(1, -2).map((l) => l.trim().split(/\s+/)[0]);
    expect(names).toEqual(["id", "label", "created_at", "updated_at"]);
  });

  it("따옴표는 설명 안에서 이스케이프된다", () => {
    const tables = [
      tbl("ts_a", [pk()], { comment: "A" }),
      tbl("ts_b", [pk(), col("a_id", "bigint", { comment: 'say "hi"' })], { comment: 'the "b" table' }),
    ];
    const source = gen(tables);
    expect(source).toContain('table ts_b "the \\"b\\" table" {');
    expect(line(source, "a_id")).toBe('a_id  bigint  ~> ts_a  "say \\"hi\\""');
  });

  it("PK 는 스냅샷이 nullable 이라 해도 널 허용으로 쓰지 않는다", () => {
    const tables = [tbl("t", [col("id", "bigint", { key: "PRI", nullable: true }), col("x", "int", { nullable: true })])];
    const source = gen(tables);
    expect(line(source, "id")).toBe("id  bigint  pk");
    expect(line(source, "x")).toBe("x  int?");
  });

  it("코멘트가 없으면 이름에서 의미를 지어내지 않는다", () => {
    const tables = [tbl("t", [pk(), col("customer_name", "varchar", { column_type: "varchar(100)" })])];
    expect(line(gen(tables), "customer_name")).toBe("customer_name  varchar(100)");
  });

  it("enc — 코멘트 근거가 있을 때만 형제 규칙이 켜지고, 좁은 컬럼은 제외", () => {
    const withEnc = tbl("t", [
      pk(),
      col("email", "varchar", { column_type: "varchar(512)", comment: "encrypted email" }),
      col("phone", "text"),
      col("nickname", "varchar", { column_type: "varchar(50)" }),
      col("address", "varchar", { column_type: "varchar(254)" }),
      col("memo", "text"),
    ]);
    const enc = gen([withEnc]);
    expect(line(enc, "email")).toBe('email  varchar(512)  enc  "encrypted email"');
    expect(line(enc, "phone")).toBe("phone  text  enc");
    expect(line(enc, "nickname")).toBe("nickname  varchar(50)");
    expect(line(enc, "address")).toBe("address  varchar(254)");
    expect(line(enc, "memo")).toBe("memo  text");

    const noEnc = tbl("t", [pk(), col("phone", "text"), col("passport_no", "varchar", { column_type: "varchar(255)" })]);
    expect(gen([noEnc])).not.toContain("enc");
  });

  it("enum — 타입의 값이 CHECK 보다 우선하고, 둘 다 없으면 쓰지 않는다", () => {
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
    const source = gen(tables);
    expect(line(source, "kind")).toBe('kind  enum  enum(A, B)  "종류"');
    expect(line(source, "state")).toBe("state  varchar(10)  enum(ON, OFF)");
    expect(line(source, "substate")).toBe("substate  varchar(10)  enum(S1, S2)");
    expect(line(source, "mode")).toBe("mode  varchar(10)");
  });

  it("다른 스키마를 가리키는 물리 FK 는 그 테이블을 external table 로 적고, 논리 추론에서도 빠진다", () => {
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
    const source = gen(tables);
    expect(source).toContain("external table `svc_identity.ts_identity_users` {");
    expect(line(source, "user_id")).toBe("user_id  bigint  -> `svc_identity.ts_identity_users`  index as ix_user_id");
    expect(generateErd(snap(tables)).notes).toEqual([]);
  });

  it("같은 스키마인데 스냅샷에 없는 테이블을 가리키면 스키마 이름 없이 external table 로 적는다", () => {
    const tables = [
      tbl("ts_child", [pk(), col("gone_id", "bigint", { key: "MUL" })], {
        fks: [{ name: "fk_gone", columns: ["gone_id"], ref_schema: "svc_booking", ref_table: "ts_gone", ref_columns: ["id"] }],
      }),
    ];
    const source = gen(tables);
    expect(source).toContain("external table ts_gone {");
    expect(line(source, "gone_id")).toBe("gone_id  bigint  -> ts_gone");
  });

  it("물리 FK 1:1 은 uk 와 -> 가 함께 간다", () => {
    const tables = [
      tbl("ts_a", [pk()], { comment: "A" }),
      tbl("ts_b", [pk(), col("a_id", "bigint", { key: "UNI" })], {
        comment: "B",
        indexes: [ix("uq_a_id", ["a_id"], true)],
        fks: [{ name: "fk_b_a", columns: ["a_id"], ref_schema: "svc_booking", ref_table: "ts_a", ref_columns: ["id"] }],
      }),
    ];
    const source = gen(tables);
    expect(line(source, "a_id")).toBe("a_id  bigint  uk as uq_a_id  -> ts_a");
    expect(compile(source).model!.relations[0].one).toBe(true);
  });

  it("식별자가 아닌 이름은 백틱으로, 여러 단어 타입은 한 단어로", () => {
    const tables = [tbl("odd table", [pk(), col("my col", "double", { column_type: "double precision", nullable: true })])];
    const source = gen(tables);
    expect(source).toContain("table `odd table` {");
    expect(line(source, "`my")).toBe("`my col`  double?");
  });
});

describe("감사 테이블", () => {
  it("revinfo 와 *_aud 는 원본 테이블의 audit envers 가 되고 따로 쓰지 않는다", () => {
    const tables = [
      tbl("ts_x", [pk(), col("status", "varchar", { column_type: "varchar(20)" }), col("amount", "decimal")]),
      tbl("revinfo", [pk("rev", "int"), col("revtstmp", "bigint", { nullable: true })]),
      tbl("ts_x_aud", [
        col("status", "varchar", { column_type: "varchar(20)", nullable: true, comment: "상태" }),
        col("rev", "int", { key: "PRI" }),
        col("revtype", "tinyint", { nullable: true }),
        col("id", "bigint", { key: "PRI" }),
      ]),
    ];
    const source = gen(tables);
    expect(source).toContain("} audit envers(status)");
    expect(source).not.toContain("table revinfo");
    expect(source).not.toContain("table ts_x_aud");
  });

  it("revinfo 가 없으면 *_aud 는 평범한 테이블로 남는다", () => {
    const tables = [tbl("ts_x", [pk()]), tbl("ts_x_aud", [col("id", "bigint", { key: "PRI" }), col("rev", "int", { key: "PRI" })])];
    const source = gen(tables);
    expect(source).toContain("table ts_x_aud {");
    expect(source).not.toContain("audit envers");
  });
});
