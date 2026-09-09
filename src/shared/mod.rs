//! md2hwpx·hwpx2md가 공유하는 단일 출처 모듈: XML 트리, 글리프 규칙,
//! 소스 방언 토큰, 오류 형식, HWPX zip 읽기 헬퍼. 각 크레이트는 re-export
//! shim으로 기존 `crate::` 경로를 그대로 쓴다.

pub mod dialect;
pub mod error;
pub mod glyph;
pub mod xmltree;
pub mod zip_read;
