/** 페이지 간 드롭 파일 전달(메모리 전용, 서버 전송 없음).
 *
 * M2hPage와 H2mPage는 App.svelte에 함께 마운트되어 있어 같은 JS 컨텍스트를
 * 공유한다. 한쪽 에디터에 다른 쪽 확장자 파일 1개가 드롭되면, 이 모듈에
 * File 객체를 맡기고 SPA 전환을 한 뒤 상대 페이지가 꺼내어 기존 흐름
 * (M2h: 에디터 채우기, H2m: convertFile)에 그대로 투입한다. 전환은 사용자
 * 드롭이라는 명시적 의도이므로 덮어쓰기 확인을 하지 않는다.
 */
export type DropTarget = 'm2h' | 'h2m'

let pending: { file: File; target: DropTarget } | null = null

export function setPendingTransfer(file: File, target: DropTarget): void {
  pending = { file, target }
}

/** 목표 페이지가 활성 전환 시 한 번만 꺼낸다. 대상이 다르면 null. */
export function takePendingTransfer(target: DropTarget): File | null {
  if (pending?.target !== target) return null
  const file = pending.file
  pending = null
  return file
}
