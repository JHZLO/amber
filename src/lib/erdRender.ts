// ERD 탭의 배치 — resin `toSvg` 에 ELK 를 웹 워커로 넘긴다. 큰 스키마(100 테이블)도 화면이 굳지 않는다.
// 같은 모델, 같은 보기면 다시 배치하지 않는다(스키마를 오가거나 보기를 되돌릴 때).

import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import { toSvg, type ElkLike, type Model, type SvgOptions, type SvgResult } from "@jhzlo/resin";
import { LayoutCache } from "@jhzlo/resin/viewer";

let elk: ElkLike | null = null;
const cache = new LayoutCache<SvgResult>(8, 48 * 1024 * 1024);

/** key 는 모델과 보기를 함께 말하는 문자열 — 같으면 같은 그림이다 */
export async function drawErd(key: string, model: Model, options: SvgOptions): Promise<SvgResult> {
  const hit = cache.get(key);
  if (hit) return hit;
  elk ??= new ELK({ workerUrl: elkWorkerUrl }) as unknown as ElkLike;
  const result = await toSvg(model, elk, options);
  cache.set(key, result);
  return result;
}
