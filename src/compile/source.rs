use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::shared::error::{err, AppError, Result};
use crate::shared::glyph::{
    bullet_item_body, classify, is_bullet_glyph, is_visible_sub_item, resolve_item_glyph,
};
use crate::compile::patterns::dialect;

static FRONT_KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][\w-]*\s*:").unwrap());
static TABLE_SEP_CELL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^:?-{3,}:?$").unwrap());
static BOX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:\*\*)?\[{1,2}(?:박스|box)\]{1,2}\s*(.*?)(?:\*\*)?$").unwrap());
static MD_HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+?)\s*#*\s*$").unwrap());
static SUB_NUM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d+\b").unwrap());
static BRACKET_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\[(?:제목|title)\]").unwrap());
static BRACKET_TITLE_STRIP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\[(?:제목|title)\]\s*[:：]?\s*").unwrap());
static GIYEOK_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^제\s*목\s*[:：]?\s*").unwrap());
static NUM_SUB_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+\.\d+)\s*(.*)$").unwrap());
static NUM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+\.)\s*(.*)$").unwrap());
static NOTE_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\\?[*※])\s+(.*)$").unwrap());
static TITLE_COMMAND_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[\[(붙임|외부제목)\]\]\s*(.*)$").unwrap());

/// 소스 명령이 변형 선택·표지 셀 텍스트를 컴파일러로 전달할 때 쓰는
/// 내부 블록 종류. lib.rs가 블록에서 꺼내 build_section 인자로 바꾼다.
pub const TITLE_VARIANT_BLOCK: &str = "title-variant";
pub const TITLE_TEXT_BLOCK: &str = "title-text";
static MD_CORE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\*\*|__)(.*?)(?:\*\*|__)$").unwrap());
static HL_BULLET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-*※]\s*").unwrap());

/// 닫는 `---`가 열린 `---`로부터 이 줄 수 안에 있어야 하이라이트 블록
/// 경계로 본다.
const HIGHLIGHT_WINDOW: usize = 5;

/// 펜스 비교용 트림. 일반 공백에 더해 웹 붙여넣기 유입 불가문자(BOM,
/// 제로폭 공백)를 걷는다 — 눈에는 `---`로 보여도 `trim()`만으로는 펜스
/// 판정에 실패해 강조 블록이 평문으로 퇴화한다. 펜스 판정에서만 쓰고
/// 본문 텍스트는 원문 보존한다.
fn fence_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\u{200b}')
}

/// 의미 블록 하나: (종류, 본문).
pub type Block = (String, String);

/// 표/박스 행 데이터를 블록 값(JSON)으로 인코딩한다.
fn encode_rows(rows: &[Vec<String>], what: &str) -> Result<String> {
    serde_json::to_string(rows).map_err(|e| AppError(format!("failed to serialize {what} data: {e}")))
}

/// UTF-8(BOM 허용) 텍스트를 줄 단위로 읽는다. CRLF는 LF로 통일.
/// 파일 I/O는 CLI 래퍼(main.rs)가 담당하고, 여기서는 문자열만 받는다.
pub fn parse_source_str_labeled(text: &str, label: &str) -> Result<(String, Vec<Block>)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines = python_splitlines(&text.replace("\r\n", "\n"));
    parse_source(&lines, label)
}

/// 에디터/Wasm용 진입점. 출처 라벨을 "editor"로 표시한다.
pub fn parse_source_str(text: &str) -> Result<(String, Vec<Block>)> {
    parse_source_str_labeled(text, "editor")
}

/// Python str.splitlines()와 같은 경계 문자 집합으로 자른다.
fn python_splitlines(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if matches!(
            ch,
            '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{1C}' | '\u{1D}' | '\u{1E}' | '\u{85}'
                | '\u{2028}' | '\u{2029}'
        ) {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `**굵게**`/`__굵게__` 한 겹을 벗겨낸다.
fn markdown_core(text: &str) -> String {
    let t = text.trim();
    match MD_CORE_RE.captures(t) {
        Some(c) => c[1].to_string(),
        None => t.to_string(),
    }
}

fn table_row(text: &str) -> Vec<String> {
    text.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// 줄 앞의 불릿 글리프와 나머지 본문. '*'는 뒤에 공백이 올 때만 목록으로
/// 본다(`**굵게**` 표식과 구별). '+'와 원문자 번호도 불릿이다.
fn bullet_split(stripped: &str) -> Option<(char, &str)> {
    let c = stripped.chars().next()?;
    if !is_bullet_glyph(c) {
        return None;
    }
    let rest = &stripped[c.len_utf8()..];
    if c == '*' && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some((c, rest))
}

/// 선행 공백/탭의 들여쓰기 폭. 탭은 4칸 탭 스톱으로 잰다.
fn indent_width(line: &str) -> usize {
    let mut col = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => col += 1,
            '\t' => col = col / 4 * 4 + 4,
            _ => break,
        }
    }
    col
}

/// 제목 후보에서 제외되는 목록성 줄인가.
fn is_list_item(stripped: &str) -> bool {
    stripped.starts_with('※') || stripped.starts_with('*') || bullet_split(stripped).is_some()
}

/// [[붙임]]/[[외부제목]] 명령 줄을 변형 이름과 셀 대체 텍스트로 푼다.
/// 이어 쓴 텍스트가 없으면 None으로, 컴파일러가 문서 제목으로 대체하게 한다.
fn title_command(stripped: &str) -> Option<(&'static str, Option<String>)> {
    let c = TITLE_COMMAND_RE.captures(stripped)?;
    let (variant, text) = match &c[1] {
        "붙임" => (dialect::ATTACH_VARIANT, &c[2]),
        _ => (dialect::EXTERNAL_VARIANT, &c[2]),
    };
    let text = text.trim();
    Some((variant, (!text.is_empty()).then(|| text.to_string())))
}

fn finish_highlight(
    highlight_lines: &mut Vec<String>,
    blocks: &mut Vec<Block>,
    label: &str,
) -> Result<()> {
    if highlight_lines.is_empty() {
        return err(format!("highlight block in {label} has no content."));
    }
    let content: Vec<String> = highlight_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| HL_BULLET_RE.replace(l.trim(), "").into_owned())
        .collect();
    highlight_lines.clear();
    if content.is_empty() {
        return err(format!("highlight block in {label} has no content."));
    }
    // 항목 경계를 보존하기 위해 JSON 배열로 인코딩한다 — 렌더러가 항목별
    // 셀 문단을 만들 때 이 경계를 사용한다(공백으로 합치면 경계가 소실된다).
    let json = serde_json::to_string(&content).map_err(|e| {
        AppError(format!("failed to serialize highlight data: {e}"))
    })?;
    blocks.push(("highlight".to_string(), json));
    Ok(())
}

/// 소스 한 파일의 해석 상태. 줄마다 종류를 판별해 대응 handler로 보내고,
/// 불릿 깊이는 루프가 끝난 뒤 상대 순위로 확정한다.
struct SourceParser<'a> {
    lines: &'a [String],
    label: &'a str,
    first: Option<usize>,
    title: String,
    fallback_title: String,
    /// 본문 해석 대상이 된 맨 첫 비어있지 않은 줄(공백 제거 상태).
    /// 제목 후보가 전혀 없을 때 최후 폴백 제목으로 쓴다.
    first_nonempty: Option<String>,
    blocks: Vec<Block>,
    /// 불릿 항목: (blocks 인덱스, 들여쓰기, 글리프)
    bullet_items: Vec<(usize, usize, char)>,
    /// 열린 박스: (blocks 인덱스, 박스 줄 들여쓰기, 모은 행들)
    box_open: Option<(usize, usize, Vec<Vec<String>>)>,
    in_front_matter: bool,
    in_highlight: bool,
    highlight_lines: Vec<String>,
    skip_until: usize,
    highlight_delimiters: HashSet<usize>,
}

impl<'a> SourceParser<'a> {
    fn new(lines: &'a [String], label: &'a str) -> Self {
        let first = lines.iter().position(|l| !l.trim().is_empty());
        let second = {
            let start = first.unwrap_or(0) + 1;
            (start..lines.len()).find(|&i| fence_trim(&lines[i]) == dialect::HRULE)
        };
        let metadata_is_front = match (first, second) {
            (Some(f), Some(s)) => {
                lines[f + 1..s].iter().any(|l| FRONT_KEY_RE.is_match(l.trim()))
            }
            _ => false,
        };
        let in_front_matter =
            first.is_some() && fence_trim(&lines[first.unwrap()]) == dialect::HRULE && metadata_is_front;
        let delimiter_start = match second {
            Some(s) if in_front_matter => s + 1,
            _ => 0,
        };
        let doc_first_dash = first.filter(|&f| fence_trim(&lines[f]) == dialect::HRULE);
        let delimiters: Vec<usize> = (delimiter_start..lines.len())
            .filter(|&i| fence_trim(&lines[i]) == dialect::HIGHLIGHT)
            .filter(|&i| Some(i) != doc_first_dash)
            .collect();
        // 쌍은 (다음 ---이 5줄 이내이고 그 사이에 빈 줄만 있는 게 아닐 때)만
        // 하이라이트 경계가 되고, 그 밖의 ---는 가로선으로 남는다. 문서 첫
        // 줄의 ---는 가로선(또는 front matter) 전용으로 절대 하이라이트
        // 경계가 되지 않는다.
        let mut highlight_delimiters = HashSet::new();
        let mut pair = 0;
        while pair + 1 < delimiters.len() {
            let (open, close) = (delimiters[pair], delimiters[pair + 1]);
            let within_window = close - open <= HIGHLIGHT_WINDOW;
            let has_content = (open + 1..close).any(|j| !lines[j].trim().is_empty());
            if within_window && has_content {
                highlight_delimiters.insert(open);
                highlight_delimiters.insert(close);
                pair += 2;
            } else {
                pair += 1;
            }
        }
        SourceParser {
            lines,
            label,
            first,
            title: String::new(),
            fallback_title: String::new(),
            first_nonempty: None,
            blocks: Vec::new(),
            bullet_items: Vec::new(),
            box_open: None,
            in_front_matter,
            in_highlight: false,
            highlight_lines: Vec::new(),
            skip_until: 0,
            highlight_delimiters,
        }
    }

    fn parse(mut self) -> Result<(String, Vec<Block>)> {
        for index in 0..self.lines.len() {
            if index < self.skip_until {
                continue;
            }
            let line = self.lines[index].trim_end_matches('\t').to_string();
            let stripped = line.trim();
            if stripped.is_empty() {
                continue;
            }
            if self.try_box_item(&line, stripped)? {
                continue;
            }
            self.close_box()?;
            if self.handle_delimiter_line(index, stripped)? {
                continue;
            }
            if self.in_front_matter {
                continue;
            }
            if self.first_nonempty.is_none() {
                self.first_nonempty = Some(stripped.to_string());
            }
            if let Some((variant, text)) = title_command(stripped) {
                self.blocks
                    .push((TITLE_VARIANT_BLOCK.to_string(), variant.to_string()));
                if let Some(text) = text {
                    self.blocks.push((TITLE_TEXT_BLOCK.to_string(), text));
                }
                continue;
            }
            if self.in_highlight {
                self.highlight_lines.push(stripped.to_string());
                continue;
            }
            if self.try_table(index, stripped)? {
                continue;
            }
            if self.try_box(&line, stripped)? {
                continue;
            }
            if self.try_markdown_heading(stripped)? {
                continue;
            }
            if self.try_title_marker(stripped) {
                continue;
            }
            if self.try_numbered_heading(stripped)? {
                continue;
            }
            if self.title.is_empty() && !is_list_item(stripped) {
                self.title = stripped.to_string();
                continue;
            }
            self.handle_body_line(&line, stripped);
        }
        self.close_box()?;
        self.finish_bullets();
        if self.title.is_empty() {
            self.title = self.fallback_title;
        }
        // 제목 후보(플레인 첫 줄, `# `, `[제목]`, 번호 제목)가 전혀 없어도
        // 오류 대신 공백을 제거한 맨 첫 줄을 제목으로 쓴다. 이때 일반 불릿
        // 글리프(`- ` 등)는 제목성을 해치므로 벗겨 내고, 본문에 남는 글리프
        // (원문자 번호·주석)는 의미 보존을 위해 그대로 둔다. 문서가 공백뿐이면
        // 여전히 오류이다.
        if self.title.is_empty() {
            let candidate = self.first_nonempty.clone().unwrap_or_default();
            self.title = match bullet_split(&candidate) {
                Some((glyph, rest)) if !classify(glyph).is_some_and(|r| r.stays_in_body()) => {
                    let body = rest.trim();
                    if body.is_empty() {
                        candidate
                    } else {
                        body.to_string()
                    }
                }
                _ => candidate,
            };
        }
        if self.title.is_empty() {
            return err(format!("no title found in {}", self.label));
        }
        if self.in_highlight {
            return err(format!("unterminated highlight block in {}", self.label));
        }
        Ok((self.title, self.blocks))
    }

    /// 구분 줄: `---`는 가로선(무시)이면서 동시에, 5줄 창 안에서 짝을
    /// 이루면 하이라이트 블록 경계다. 줄을 소비했으면 true.
    fn handle_delimiter_line(&mut self, index: usize, stripped: &str) -> Result<bool> {
        match fence_trim(stripped) {
            // `---`는 가로선(무시)이면서 동시에, 5줄 창 안에서 짝을 이루면
            // 하이라이트 경계다. front matter 닫기도 이 줄이 담당한다.
            dialect::HRULE => {
                if self.in_front_matter && Some(index) != self.first {
                    self.in_front_matter = false;
                }
                if self.highlight_delimiters.contains(&index) {
                    if self.in_highlight {
                        finish_highlight(&mut self.highlight_lines, &mut self.blocks, self.label)?;
                        self.in_highlight = false;
                    } else {
                        self.in_highlight = true;
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Markdown 표 헤더+구분선 쌍이면 이후 행을 모아 table 블록으로 push.
    fn try_table(&mut self, index: usize, stripped: &str) -> Result<bool> {
        if !stripped.contains('|') || index + 1 >= self.lines.len() {
            return Ok(false);
        }
        let header = table_row(stripped);
        let separator = table_row(&self.lines[index + 1]);
        if header.len() <= 1
            || separator.len() != header.len()
            || !separator.iter().all(|c| TABLE_SEP_CELL_RE.is_match(c))
        {
            return Ok(false);
        }
        let mut rows = vec![header];
        let mut end = index + 2;
        while end < self.lines.len() {
            let t = self.lines[end].trim();
            if t.is_empty() || !t.contains('|') {
                break;
            }
            rows.push(table_row(t));
            end += 1;
        }
        let json = encode_rows(&rows, "table")?;
        self.blocks.push(("table".to_string(), json));
        self.skip_until = end;
        Ok(true)
    }

    /// `[박스] ...` / `[box] ...` 줄. 제목은 JSON 첫 행으로 저장하고, 뒤따르는
    /// 들여쓰기 불릿 항목은 box_open 상태로 모은다.
    fn try_box(&mut self, line: &str, stripped: &str) -> Result<bool> {
        match BOX_RE.captures(stripped) {
            Some(c) => {
                let rows = vec![vec![c[1].trim().to_string()]];
                let json = encode_rows(&rows, "box")?;
                self.blocks.push(("box".to_string(), json));
                self.box_open = Some((self.blocks.len() - 1, indent_width(line), rows));
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 박스 줄 아래의 들여쓰기된 불릿 항목 줄을 열린 박스에 모은다.
    fn try_box_item(&mut self, line: &str, stripped: &str) -> Result<bool> {
        let Some((_, box_indent, rows)) = self.box_open.as_mut() else {
            return Ok(false);
        };
        // 가로선은 '-' 불릿 항목이 아니라 구분선으로 다룬다.
        if fence_trim(stripped) == dialect::HRULE {
            return Ok(false);
        }
        let Some((glyph, rest)) = bullet_split(stripped) else {
            return Ok(false);
        };
        if indent_width(line) <= *box_indent {
            return Ok(false);
        }
        rows.push(vec![bullet_item_body(glyph, rest)]);
        Ok(true)
    }

    /// 열린 박스를 닫는다. 모은 행이 있으면 블록 값을 갈아끼운다.
    fn close_box(&mut self) -> Result<()> {
        let Some((index, _, rows)) = self.box_open.take() else {
            return Ok(());
        };
        if rows.len() > 1 {
            self.blocks[index].1 = encode_rows(&rows, "box")?;
        }
        Ok(())
    }

    /// `# ` 마크다운 제목. 첫 `#`은 문서 제목, `## N. ` 번호 헤딩은 heading,
    /// `### `는 heading_sub, 그 밖의 `##`는 heading_default 블록이다.
    /// 제목이 이미 정해진 뒤의 `#` 줄은 일반 본문 경로로 흘려보낸다.
    fn try_markdown_heading(&mut self, stripped: &str) -> Result<bool> {
        let Some(c) = MD_HEADING_RE.captures(stripped) else {
            return Ok(false);
        };
        let level = c[1].len();
        let heading_text = c[2].trim().to_string();
        if level == 1 {
            if self.title.is_empty() {
                self.title = heading_text;
                return Ok(true);
            }
            return Ok(false);
        }
        let core = markdown_core(&heading_text);
        // 판정 순서는 try_numbered_heading과 같다: 복합 번호(`2.1`)가 단일
        // 번호(`2.`)보다 먼저다. `## N. 제목`은 평문 `N. 제목`과 같은 heading
        // 블록(번호 보존), `### `는 모든 텍스트를 heading_sub로 보낸다.
        let kind = if level == 3 || SUB_NUM_RE.is_match(&core) {
            "heading_sub"
        } else if level == 2 && NUM_RE.is_match(&core) {
            "heading"
        } else {
            "heading_default"
        };
        if kind == "heading" && self.fallback_title.is_empty() {
            if let Some(nc) = NUM_RE.captures(&core) {
                self.fallback_title = nc[2].trim().to_string();
            }
        }
        self.blocks.push((kind.to_string(), heading_text));
        Ok(true)
    }

    /// `[제목]`/`[title]` 또는 `제목:` 관례 표기.
    fn try_title_marker(&mut self, stripped: &str) -> bool {
        if !self.title.is_empty() {
            return false;
        }
        if BRACKET_TITLE_RE.is_match(stripped) {
            self.title = BRACKET_TITLE_STRIP_RE.replace(stripped, "").into_owned();
            return true;
        }
        if stripped.starts_with("제목") || stripped.starts_with("제 목") {
            self.title = GIYEOK_TITLE_RE.replace(stripped, "").into_owned();
            return true;
        }
        false
    }

    /// `2.1` 형식 소제목 또는 `1.` 번호 제목 줄.
    fn try_numbered_heading(&mut self, stripped: &str) -> Result<bool> {
        if NUM_SUB_RE.is_match(&markdown_core(stripped)) {
            self.blocks.push(("heading_sub".to_string(), stripped.to_string()));
            return Ok(true);
        }
        if let Some(c) = NUM_RE.captures(&markdown_core(stripped)) {
            self.blocks.push(("heading".to_string(), stripped.to_string()));
            if self.fallback_title.is_empty() {
                self.fallback_title = c[2].trim().to_string();
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// 제목 게이트를 통과한 본문 줄: 각주/주석/불릿/일반 문단.
    fn handle_body_line(&mut self, line: &str, stripped: &str) {
        let indent = indent_width(line);
        // ※ 각주는 들여쓰기와 무관하게 reference다.
        if let Some(rest) = stripped.strip_prefix('※') {
            self.blocks.push(("reference".to_string(), rest.trim_start().to_string()));
            return;
        }
        let Some((glyph, rest)) = bullet_split(stripped) else {
            self.blocks.push(("depth1".to_string(), stripped.to_string()));
            return;
        };
        if glyph == '*' && indent > 0 {
            // 들여쓴 '*'는 명시적 보조표기(asterisk)다.
            self.blocks.push(("asterisk".to_string(), rest.trim_start().to_string()));
            return;
        }
        // `- \* ...` / `- ※ ...` 주석 표기는 기존 규칙 그대로다.
        if glyph == '-' {
            if let Some(nc) = NOTE_MARKER_RE.captures(rest.trim()) {
                let kind = if nc[1].ends_with('*') { "asterisk" } else { "reference" };
                self.blocks.push((kind.to_string(), nc[2].trim().to_string()));
                return;
            }
        }
        // 주석 글리프와 원문자 번호는 본문 앞에 남는다. 번호는 그대로 출력
        // 불릿이 되고 주석 글리프는 출력 불릿 없이 본문 일부로 나온다.
        let body = bullet_item_body(glyph, rest);
        let glyph = resolve_item_glyph(glyph, &body);
        // 깊이는 루프 후 상대 순위로 확정하고, 여기서는 자리만 남긴다.
        self.blocks.push((String::new(), body));
        self.bullet_items.push((self.blocks.len() - 1, indent, glyph));
    }

    /// 모은 불릿 항목의 깊이를 확정한다. 서로 다른 들여쓰기 값을 오름차순으로
    /// 세워 상대 순위를 매긴다(가장 얕은 들여쓰기가 depth1, 최대 depth3).
    /// "보이는 하위 항목" 글리프(○/ㅇ, 원문자 번호, 주석 글리프)는 들여쓰기
    /// 대신 보이는 모양으로 쓰는 문서 호환을 위해, 같은 들여쓰기의 다른 기호
    /// 바로 아래에서는 한 단계 더 깊게 보고 연달아 오면 같은 단계를 유지한다.
    fn finish_bullets(&mut self) {
        if self.bullet_items.is_empty() {
            return;
        }
        let mut ranks: Vec<usize> = self.bullet_items.iter().map(|&(_, i, _)| i).collect();
        ranks.sort_unstable();
        ranks.dedup();
        let mut prev: Option<(usize, u8, char)> = None;
        for &(bi, indent, glyph) in &self.bullet_items {
            let rank = ranks
                .iter()
                .position(|&i| i == indent)
                .expect("들여쓰기 값은 ranks에 있음") as u8;
            let mut depth = (rank + 1).min(3);
            if is_visible_sub_item(glyph) {
                if let Some((pi, pd, pg)) = prev {
                    if pi == indent {
                        depth = if is_visible_sub_item(pg) {
                            pd
                        } else {
                            (depth + 1).min(3)
                        };
                    }
                }
            }
            self.blocks[bi].0 = format!("depth{depth}");
            prev = Some((indent, depth, glyph));
        }
    }
}

/// 이메일 인용 표기(`>`, `>>`, `> >`)를 줄 앞에서 걷어낸다. 들여쓰기는
/// 보존해 불릿 깊이 판정이 바뀌지 않게 한다.
fn strip_quote_markers(line: &str) -> String {
    let body = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - body.len()];
    let mut rest = body;
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    if rest.len() == body.len() {
        line.to_string()
    } else {
        format!("{indent}{rest}")
    }
}

/// 소스 파일을 제목과 의미 블록(heading/depth/note/box/highlight/table)으로
/// 해석한다. hwp.py의 parse_source와 동일한 규칙 위에 불릿 깊이는 상대
/// 들여쓰기로 판정한다.
pub fn parse_source(lines: &[String], source_label: &str) -> Result<(String, Vec<Block>)> {
    let lines: Vec<String> = lines.iter().map(|l| strip_quote_markers(l)).collect();
    SourceParser::new(&lines, source_label).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn parses_title_headings_and_depths() {
        let src = lines("제목 : 테스트 보고서\n\n1. 배경\n- 항목 하나\n  - (하위) 내용\n    - 깊은 항목\n※ 각주");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "테스트 보고서");
        assert_eq!(blocks[0], ("heading".to_string(), "1. 배경".to_string()));
        assert_eq!(blocks[1].0, "depth1");
        assert_eq!(blocks[2].0, "depth2");
        assert_eq!(blocks[3].0, "depth3");
        assert_eq!(blocks[4].0, "reference");
    }

    #[test]
    fn parses_markdown_title_and_box() {
        let src = lines("# 굵은 제목\n\n[박스] 요약 문장");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "굵은 제목");
        assert_eq!(blocks[0], ("box".to_string(), r#"[["요약 문장"]]"#.to_string()));
    }

    #[test]
    fn markdown_heading_levels_map_to_block_kinds() {
        // `## N. 제목`은 평문 `N. 제목`과 같은 heading 블록, `### `는 모든
        // 텍스트를 heading_sub로, 복합 번호 `## N.M`은 기존처럼 heading_sub,
        // 그 밖의 `##`/`####`는 heading_default다.
        let src = lines(
            "제목: T\n## 1. 개요\n### 1 세부\n## 2.1 복합\n## 일반 제목\n#### 4단 제목",
        );
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0], ("heading".to_string(), "1. 개요".to_string()));
        assert_eq!(blocks[1], ("heading_sub".to_string(), "1 세부".to_string()));
        assert_eq!(blocks[2], ("heading_sub".to_string(), "2.1 복합".to_string()));
        assert_eq!(blocks[3], ("heading_default".to_string(), "일반 제목".to_string()));
        assert_eq!(blocks[4], ("heading_default".to_string(), "4단 제목".to_string()));
    }

    #[test]
    fn markdown_numbered_heading_sets_fallback_title() {
        // 제목 줄이 없으면 `## N. 제목`의 본문이 표지 제목 후보가 된다
        // (평문 `N. 제목`의 try_numbered_heading 규칙과 동일).
        let src = lines("## 1. 개요\n- 항목");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "개요");
        assert_eq!(blocks[0], ("heading".to_string(), "1. 개요".to_string()));
    }

    #[test]
    fn box_collects_following_indented_bullets() {
        // [박스] 줄 아래 더 깊이 들여쓰인 불릿 줄은 박스 항목이고, 같은
        // 들여쓰기 이하의 불릿이 나오면 박스는 닫힌다.
        let src = lines(
            "제목: T\n[박스] 요약\n  - 첫 항목\n  * 둘째 항목\n- 일반 항목",
        );
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "box");
        let rows: Vec<Vec<String>> = serde_json::from_str(&blocks[0].1).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0], "요약");
        assert_eq!(rows[1][0], "첫 항목");
        assert_eq!(rows[2][0], "둘째 항목");
        // 박스가 닫힌 뒤의 불릿은 여전히 깊이 항목이다.
        assert_eq!(blocks[1].0, "depth1");
        assert_eq!(blocks[1].1, "일반 항목");
    }

    #[test]
    fn box_items_keep_body_glyphs_and_bold() {
        let src = lines("제목: T\n[박스] 요약\n  - **굵게** 항목\n  ① 번호 항목\n  ☞ 주석 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        let rows: Vec<Vec<String>> = serde_json::from_str(&blocks[0].1).unwrap();
        assert_eq!(rows[1][0], "**굵게** 항목");
        // 원문자 번호와 주석 글리프는 본문 앞에 남는다(깊이 항목과 같은 규칙).
        assert_eq!(rows[2][0], "① 번호 항목");
        assert_eq!(rows[3][0], "☞ 주석 항목");
    }

    #[test]
    fn table_block_is_encoded_as_json() {
        let src = lines("제목: T\n| 열1 | 열2 |\n| --- | --- |\n| a | b |");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "table");
        assert_eq!(blocks[0].1, r#"[["열1","열2"],["a","b"]]"#);
    }

    #[test]
    fn dash_wraps_highlight_block_within_window() {
        // highlight 경계는 `---`다. 닫는 `---`가 열린 `---`로부터 5줄
        // 이내일 때만 경계로 본다. 항목 경계는 JSON 배열로 보존된다 —
        // 렌더러가 항목별 셀 문단을 만드는 데 쓰인다.
        let src = lines("제목: T\n---\n- 핵심 요약 문장\n---\n- 일반 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(
            blocks[0],
            ("highlight".to_string(), r#"["핵심 요약 문장"]"#.to_string())
        );
        assert_eq!(blocks[1].0, "depth1");
        assert_eq!(blocks[1].1, "일반 항목");

        let multi = lines("제목: T\n---\n- 첫 항목\n- 둘째 항목\n---\n- 일반 항목");
        let (_, blocks) = parse_source(&multi, "테스트").unwrap();
        assert_eq!(
            blocks[0],
            (
                "highlight".to_string(),
                r#"["첫 항목","둘째 항목"]"#.to_string()
            )
        );
    }

    #[test]
    fn invisible_paste_chars_on_fences_still_pair_highlight() {
        // 웹 붙여넣기로 묻은 BOM·제로폭 공백이 펜스 앞뒤에 있어도 짝을
        // 이룬다. 눈에는 `---`로 보여 평문 퇴화하면 안 된다.
        let src = lines("제목: T\n\u{feff}---\n- 핵심 요약 문장\n---\u{200b}\n- 일반 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(
            blocks[0],
            ("highlight".to_string(), r#"["핵심 요약 문장"]"#.to_string())
        );
        assert_eq!(blocks[1].0, "depth1");
    }

    #[test]
    fn horizontal_rule_dashes_are_ignored() {
        // 짝을 이루지 못한 `---`는 가로선으로 보아 highlight에 쓰지 않고
        // 무시한다.
        let src = lines("제목: T\n1. 개요\n---\n- 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "heading");
        assert_eq!(blocks[1].0, "depth1");
        assert_eq!(blocks[1].1, "항목");
    }

    #[test]
    fn dash_pair_beyond_window_is_horizontal_rule() {
        // 닫는 `---`가 5줄 창을 벗어나면(거리 6) 하이라이트가 아니라
        // 가로선 두 개다. 대시가 없을 때와 같은 여섯 개 불릿이 나온다.
        let src = lines("제목: T\n---\n- a\n- b\n- c\n- d\n- e\n---\n- 일반");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert!(!blocks.iter().any(|b| b.0 == "highlight"));
        assert_eq!(blocks.len(), 6);
        assert!(blocks.iter().all(|b| b.0 == "depth1"));
        assert_eq!(blocks[5].1, "일반");
    }

    #[test]
    fn equals_is_now_ordinary_text() {
        // `===`는 더 이상 방언 토큰이 아니고 일반 본문 줄이다.
        let src = lines("제목: T\n===\n- 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(
            blocks,
            vec![
                ("depth1".to_string(), "===".to_string()),
                ("depth1".to_string(), "항목".to_string())
            ]
        );
    }

    #[test]
    fn adjacent_dashes_are_horizontal_rules() {
        // 붙어 있는 `---` 두 줄은 그 사이에 내용이 없어 하이라이트가 되지
        // 않는다(가로선 두 개).
        let src = lines("제목: T\n---\n---\n- 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(
            blocks,
            vec![("depth1".to_string(), "항목".to_string())]
        );
    }

    #[test]
    fn blank_only_between_dashes_is_horizontal_rule() {
        // `---` 사이에 빈 줄만 있으면 하이라이트가 아니라 가로선 두 개다.
        let src = lines("제목: T\n---\n\n---\n- 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(
            blocks,
            vec![("depth1".to_string(), "항목".to_string())]
        );
    }

    #[test]
    fn doc_leading_dash_is_never_highlight() {
        // 문서 첫 줄의 `---`는 가로선(또는 front matter) 전용이다.
        let src = lines("---\n# 문서 제목\n1. 개요\n---\n본문");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "문서 제목");
        assert_eq!(
            blocks,
            vec![
                ("heading".to_string(), "1. 개요".to_string()),
                ("depth1".to_string(), "본문".to_string())
            ]
        );
    }

    #[test]
    fn window_boundary_at_five_lines_is_highlight() {
        // 경계 거리가 정확히 5줄이면 창 안이다(5 이하 포함).
        let src = lines("제목: T\n---\n- a\n- b\n- c\n- d\n---\n- 일반");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "highlight");
        let items: Vec<String> = serde_json::from_str(&blocks[0].1).unwrap();
        assert_eq!(items, vec!["a", "b", "c", "d"]);
        assert_eq!(blocks[1].0, "depth1");
        assert_eq!(blocks[1].1, "일반");
    }

    #[test]
    fn window_distance_four_is_highlight() {
        // 경계 거리 4(내용 3줄)도 창 안이다.
        let src = lines("제목: T\n---\n- a\n- b\n- c\n---");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "highlight");
        let items: Vec<String> = serde_json::from_str(&blocks[0].1).unwrap();
        assert_eq!(items, vec!["a", "b", "c"]);
    }

    #[test]
    fn front_matter_fence_still_uses_dashes() {
        let src = lines("---\nkey: value\n---\n제목: T\n- 항목");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "T");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[0].1, "항목");
    }

    #[test]
    fn quote_markers_are_stripped_from_line_starts() {
        // 이메일 인용 '>'는 줄 앞에서 걷어내 본문 앞에 남지 않게 한다.
        // '>>'와 '> >'도 같은 취급이고, '>' 뒤 들여쓰기는 깊이 판정에 보존한다.
        let src = lines("> 제목: T\n> - (키워드) 인용 항목\n>> - 겹인용 항목\n>   - 들여쓰기 인용");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "T");
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[0].1, "(키워드) 인용 항목");
        assert_eq!(blocks[1].0, "depth1");
        assert_eq!(blocks[1].1, "겹인용 항목");
        assert_eq!(blocks[2].0, "depth2");
        assert_eq!(blocks[2].1, "들여쓰기 인용");
    }

    #[test]
    fn quoted_table_is_parsed() {
        let src = lines("> 제목: T\n> | 열1 | 열2 |\n> | --- | --- |\n> | a | b |");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "table");
        assert_eq!(blocks[0].1, r#"[["열1","열2"],["a","b"]]"#);
    }

    #[test]
    fn double_bracket_box_command_and_legacy_single() {
        // [[박스]]가 표준이고 [박스] 단일 괄호도 하위호환으로 인식한다.
        let src = lines("제목: T\n[[박스]] 요약\n  - 항목\n[박스] 레거시");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "box");
        let rows: Vec<Vec<String>> = serde_json::from_str(&blocks[0].1).unwrap();
        assert_eq!(rows[0][0], "요약");
        assert_eq!(rows[1][0], "항목");
        assert_eq!(blocks[1].0, "box");
        let rows: Vec<Vec<String>> = serde_json::from_str(&blocks[1].1).unwrap();
        assert_eq!(rows[0][0], "레거시");
    }

    #[test]
    fn attach_command_selects_variant_and_title() {
        // [[붙임]]은 attachment 변형 선택이고 이어 쓴 제목은 표지 셀 텍스트다.
        let src = lines("제목: T\n[[붙임]] 별지 제목\n- 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(
            blocks[0],
            (TITLE_VARIANT_BLOCK.to_string(), "attachment".to_string())
        );
        assert_eq!(
            blocks[1],
            (TITLE_TEXT_BLOCK.to_string(), "별지 제목".to_string())
        );
        assert_eq!(blocks[2].0, "depth1");
    }

    #[test]
    fn attach_command_without_text_skips_title_text() {
        // 이어 쓴 텍스트가 없으면 변형만 선택하고 문서 제목이 대체한다.
        let src = lines("제목: T\n[[붙임]]\n- 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, TITLE_VARIANT_BLOCK);
        assert_eq!(blocks[1].0, "depth1");
    }

    #[test]
    fn external_command_selects_external() {
        let src = lines("제목: T\n[[외부제목]] 외부 배포용 문서");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(
            blocks[0],
            (TITLE_VARIANT_BLOCK.to_string(), "external".to_string())
        );
        assert_eq!(
            blocks[1],
            (TITLE_TEXT_BLOCK.to_string(), "외부 배포용 문서".to_string())
        );
    }

    #[test]
    fn command_line_is_never_the_document_title() {
        // 명령 줄은 제목 후보가 아니고, 문서 제목은 별도로 인식한다.
        let src = lines("[[붙임]] 별지\n# 실제 보고서 제목");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "실제 보고서 제목");
        assert_eq!(blocks[0].0, TITLE_VARIANT_BLOCK);
    }

    #[test]
    fn missing_title_falls_back_to_first_nonempty_line() {
        // 제목 후보(플레인 첫 줄, `# `, `[제목]`, 번호 제목)가 전혀 없는
        // 불릿뿐 문서는 오류 대신 공백을 제거한 맨 첫 줄에서 일반 불릿
        // 글리프를 벗겨 제목으로 쓴다.
        let src = lines("- 항목");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "항목");
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[0].1, "항목");

        // 본문에 남는 글리프(원문자 번호)는 제목에서도 유지한다.
        let src = lines("① 개요\n- 내용");
        let (title, _) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "① 개요");
    }

    #[test]
    fn whitespace_only_source_is_still_an_error() {
        let src = lines("  \n\t\n");
        assert!(parse_source(&src, "테스트").is_err());
    }

    #[test]
    fn circle_and_ieung_after_sibling_are_depth2() {
        // 같은 들여쓰기의 다른 기호 뒤에 오는 ○/ㅇ는 "보이는 하위 불릿"으로
        // 한 단계 아래(depth2)로 본다(들여쓰기 없이 depth2를 쓰는 문서 호환).
        let src = lines("제목: T\n- 상위 항목\n○ 공백 동그라미\nㅇ 한글 자음");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "T");
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "공백 동그라미");
        assert_eq!(blocks[2].0, "depth2");
        assert_eq!(blocks[2].1, "한글 자음");
    }

    #[test]
    fn bullet_glyphs_are_free_and_depth_is_relative() {
        // 불릿 기호는 한정되지 않고, 깊이는 상대적 들여쓰기로 판정한다.
        let src = lines("제목: T\n□ 첫째\n  ● 둘째\n    • 셋째\n□ 다시 첫째\n- 또 첫째");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[2].0, "depth3");
        assert_eq!(blocks[3].0, "depth1");
        assert_eq!(blocks[4].0, "depth1");
    }

    #[test]
    fn tab_indented_bullets_rank_relative() {
        // 탭 들여쓰기도 상대 순위로 판정한다. 첫 항목이 들여쓰기로
        // 시작해도 가장 얕은 단계가 depth1이다.
        let src = lines("제목: T\n\t□ 첫째\n\t\t● 둘째\n\t□ 다시 첫째");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[2].0, "depth1");
    }

    #[test]
    fn double_asterisk_line_is_not_a_bullet_nor_title() {
        // '**굵게**' 표식 줄은 목록도 제목도 아니다(원본 동작 유지).
        let src = lines("**굵은 본문**\n\n제목: T");
        let (title, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "T");
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[0].1, "**굵은 본문**");
    }

    #[test]
    fn plus_sign_is_a_bullet_glyph() {
        // '+'도 불릿이다. 본문에는 남지 않고 템플릿 불릿으로 정규화된다.
        let src = lines("제목: T\n- 상위\n  + (’20.5월) 하위 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[0].1, "상위");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "(’20.5월) 하위 항목");
    }

    #[test]
    fn plus_sign_needs_no_trailing_space() {
        let src = lines("제목: T\n- 상위\n  +(’20.5월) 하위 항목");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "(’20.5월) 하위 항목");
    }

    #[test]
    fn circled_number_is_a_bullet_and_stays_in_body() {
        // 원문자 번호는 불릿 글리프다. 출력 때 템플릿 불릿 대신 그 번호를
        // 찍어야 하므로 본문 앞에 남아 있어야 한다.
        let src = lines("제목: T\n- 상위\n  ① (인력수요) 내용");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "① (인력수요) 내용");
    }

    #[test]
    fn circled_number_in_body_promotes_to_marker_input() {
        // 불릿 본문이 원문자 번호로 시작해도 같은 규칙이다.
        let src = lines("제목: T\n- 상위\n  - ① (인력수요) 내용");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "① (인력수요) 내용");
    }

    #[test]
    fn circled_number_after_sibling_is_depth2() {
        // ○/ㅇ와 같은 "보이는 하위 불릿" 호환 규칙을 원문자 번호에도 적용한다.
        let src = lines("제목: T\n- 상위 항목\n① 하위 항목\n② 다음 하위");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[2].0, "depth2");
        assert_eq!(blocks[1].1, "① 하위 항목");
    }

    #[test]
    fn arrow_note_line_takes_relative_depth() {
        // ☞ 주석 줄은 들여쓰기 순위대로 깊이를 정하고, 본문 앞에 ☞를
        // 남긴다(레ンダ러가 이를 보고 출력 불릿을 비운다).
        let src = lines("제목: T\n□ 첫째 항목\n  ☞ (사업영향) 내용");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "☞ (사업영향) 내용");
    }

    #[test]
    fn arrow_note_after_sibling_is_deepened_and_stays() {
        // 들여쓰기 없이 바로 위 항목 아래에서 오는 ☞는 한 단계 더 깊게
        // 보고, ☞가 연달아 오면 같은 단계를 유지한다(○/ㅇ 호환 규칙과 같다).
        let src = lines("제목: T\n□ 첫째 항목\n☞ (사업영향) 내용\n☞ 다음 노트");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[2].0, "depth2");
        assert_eq!(blocks[2].1, "☞ 다음 노트");
    }

    #[test]
    fn arrow_note_in_box_body_loses_the_box_glyph() {
        // '□  ☞ ...'처럼 박스 글리프 뒤에 ☞가 오면 박스는 사라지고 한 단계
        // 아래 깊이로, 본문은 ☞부터 시작한다.
        let src = lines("제목: T\n□ 첫째 항목\n□  ☞ (사업영향) 내용");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "☞ (사업영향) 내용");
    }

    #[test]
    fn arrow_note_line_is_not_a_title_gate_candidate() {
        // ☞ 주석 줄은 제목 후보 게이트를 통과하지 않지만, 제목이 전혀 없는
        // 문서는 최후 폴백으로 공백을 제거한 맨 첫 줄을 제목으로 컴파일된다.
        let src = lines("☞ 노트");
        let (title, _) = parse_source(&src, "테스트").unwrap();
        assert_eq!(title, "☞ 노트");
    }

    #[test]
    fn glyph_after_box_glyph_decides_the_item() {
        // 박스 등 다른 기호 뒤에 주석/번호 글리프가 붙어 오면 그 글리프가
        // 항목의 성격을 결정한다: 보이는 하위 항목이므로 한 단계 더 깊게.
        let src = lines("제목: T\n□ 상위 항목\n□ ① 하위 항목\n□  ☞ 주석");
        let (_, blocks) = parse_source(&src, "테스트").unwrap();
        assert_eq!(blocks[0].0, "depth1");
        assert_eq!(blocks[1].0, "depth2");
        assert_eq!(blocks[1].1, "① 하위 항목");
        assert_eq!(blocks[2].0, "depth2");
        assert_eq!(blocks[2].1, "☞ 주석");
    }
}
