// @vitest-environment jsdom
//
// 공용 Modal 의 닫기 규약 회귀 테스트.
// - 배경판을 눌러도 닫히지 않는다(AI 작성 지시가 창 밖 클릭 한 번으로 날아갔다)
// - Esc 는 입력한 게 없으면 바로 닫고, 입력했으면 한 번 더 묻는다
// - 확인 창이 떠 있을 때 Esc 는 확인 창만 닫는다(아래 모달까지 같이 닫히면 안 된다)
// - 삭제 확인은 열리자마자 '취소'에 포커스가 간다(Enter 두 번으로 지워지지 않게)
// - 확인 창이 닫히면 열기 전 자리(쓰던 입력칸)로 포커스가 돌아온다

import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConfirmDelete, Modal } from "./ui";
import { t } from "./lib/i18n";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | null = null;

function mount(onClose: () => void) {
  const host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  act(() => {
    root!.render(
      createElement(Modal, {
        open: true,
        title: "제목",
        onClose,
        children: createElement("textarea", { className: "probe" }),
      }),
    );
  });
}

const overlays = () => document.querySelectorAll(".overlay");
// 실제 키 입력처럼 포커스된 요소에서 쏜다 — document(Radix 레이어)와 window(우리 핸들러)까지 올라간다
const pressEsc = () =>
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );
  });
const typeInto = (el: HTMLTextAreaElement, value: string) =>
  act(() => {
    el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
const buttonWithText = (text: string) =>
  [...document.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent === text,
  );

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  document.body.innerHTML = "";
});

describe("Modal 닫기 규약", () => {
  it("배경판을 눌러도 닫히지 않는다", () => {
    const onClose = vi.fn();
    mount(onClose);
    act(() => {
      overlays()[0].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
      overlays()[0].dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
      overlays()[0].dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onClose).not.toHaveBeenCalled();
    expect(overlays()).toHaveLength(1);
  });

  it("입력한 게 없으면 Esc 로 바로 닫힌다", () => {
    const onClose = vi.fn();
    mount(onClose);
    pressEsc();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("입력했으면 Esc 에서 한 번 더 묻고, 버리기를 눌러야 닫힌다", () => {
    const onClose = vi.fn();
    mount(onClose);
    typeInto(document.querySelector<HTMLTextAreaElement>(".probe")!, "쓰던 지시");
    pressEsc();
    expect(onClose).not.toHaveBeenCalled();
    expect(overlays()).toHaveLength(2);
    expect(document.body.textContent).toContain(t("common.closeDirty.title"));

    act(() => buttonWithText(t("common.closeDirty.discard"))!.click());
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("확인 창에서 Esc 는 '계속 편집' — 아래 모달은 그대로 남는다", () => {
    const onClose = vi.fn();
    mount(onClose);
    const box = document.querySelector<HTMLTextAreaElement>(".probe")!;
    typeInto(box, "쓰던 지시");
    pressEsc();
    expect(overlays()).toHaveLength(2);

    pressEsc();
    expect(overlays()).toHaveLength(1);
    expect(onClose).not.toHaveBeenCalled();
    expect(box.value).toBe("쓰던 지시");
  });

  it("확인 창을 닫으면 쓰던 입력칸으로 포커스가 돌아온다", async () => {
    mount(vi.fn());
    const box = document.querySelector<HTMLTextAreaElement>(".probe")!;
    act(() => box.focus());
    typeInto(box, "쓰던 지시");
    pressEsc();
    expect(document.activeElement?.textContent).toBe(t("common.unsaved.keep"));
    pressEsc();
    // Radix 는 닫힌 다음 틱에 포커스를 돌려준다
    await act(() => new Promise((r) => setTimeout(r, 0)));
    expect(document.activeElement).toBe(box);
  });

  it("X 는 누른 것 자체가 뜻이라 묻지 않고 닫는다", () => {
    const onClose = vi.fn();
    mount(onClose);
    typeInto(document.querySelector<HTMLTextAreaElement>(".probe")!, "쓰던 지시");
    act(() =>
      document
        .querySelector<HTMLButtonElement>(`.modal-head button[aria-label="${t("common.close")}"]`)!
        .click(),
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("삭제 확인", () => {
  it("열리면 취소에 포커스가 가고, Esc 는 취소다", () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    const host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    act(() => {
      root!.render(
        createElement(ConfirmDelete, {
          open: true,
          title: "삭제",
          name: "메모",
          body: "{name} 을 지울까요?",
          onCancel,
          onConfirm,
        }),
      );
    });
    expect(document.activeElement?.textContent).toBe(t("common.cancel"));
    pressEsc();
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
