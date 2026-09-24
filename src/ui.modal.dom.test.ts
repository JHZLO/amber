// @vitest-environment jsdom
//
// 공용 Modal 의 닫기 규약 회귀 테스트.
// - 배경판을 눌러도 닫히지 않는다(AI 작성 지시가 창 밖 클릭 한 번으로 날아갔다)
// - Esc 는 입력한 게 없으면 바로 닫고, 입력했으면 한 번 더 묻는다
// - 확인 창이 떠 있을 때 Esc 는 확인 창만 닫는다(아래 모달까지 같이 닫히면 안 된다)

import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Modal } from "./ui";
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
const pressEsc = () =>
  act(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
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
