//! 소스 방언의 리터럴 토큰 — common 크레이트의 단일 출처를 re-export한다.
//! hwpx2md가 내보내는 토큰이 md2hwpx의 소스 파서(`source.rs`)가 다시 먹는
//! 토큰과 같아야 한다는 요구는 common::dialect가 보증한다.
pub mod dialect {
    #[allow(unused_imports)]
    pub use crate::shared::dialect::*;
}
