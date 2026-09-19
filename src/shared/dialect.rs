//! 소스 방언의 리터럴 토큰의 단일 출처. 문법을 바꿀 때는 이 값들과
//! prompt.md·example.md·README의 문법 문서를 함께 고쳐야 하며,
//! md2hwpx의 `dialect_tokens_are_documented` 테스트가 빠뜨린 문서를 잡아준다.
pub mod dialect {
    /// `[[highlight]]` 블록의 경계. 가로선(`---`)과 같은 문자열을 쓰며,
    /// 컴파일러의 5줄 창 규칙(닫는 `---`가 열린 `---`로부터 5줄 이내)로
    /// 구분한다. 쌍으로 감싼 목록만 강조 블록이 된다.
    pub const HIGHLIGHT: &str = "---";

    /// 가로선. 출력에서 무시되고 front matter 펜스로도 쓰인다. 하이라이트
    /// 경계와 문자열을 공유하므로 실제 구분은 5줄 창 규칙이 담당한다.
    pub const HRULE: &str = "---";

    /// 박스 명령. 이어 쓴 제목이 박스 첫 행이 된다. 단일 괄호 `[박스]`도 허용.
    pub const BOX: &str = "[[박스]]";

    /// 붙임 표지 명령. 이어 쓴 제목이 표지 3번째 셀을 대체한다.
    pub const ATTACH: &str = "[[붙임]]";

    /// 외부용 표지 명령. 이어 쓴 내용이 표지 첫 셀을 대체한다.
    pub const EXTERNAL: &str = "[[외부제목]]";

    /// `[[붙임]]`이 선택하는 템플릿 title 변형 이름.
    pub const ATTACH_VARIANT: &str = "attachment";

    /// `[[외부제목]]`이 선택하는 템플릿 title 변형 이름.
    pub const EXTERNAL_VARIANT: &str = "external";
}

// 중첩 모듈 형태를 유지하되 common::dialect::* 로도 상수에 바로 닿게
// 한다(각 크레이트의 shim이 `pub use shared::dialect::*`를 글로브로 가져간다).
pub use dialect::*;
