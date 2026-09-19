//! CLI가 stdout에 JSON으로 출력하는 변환 보고서.
//! md2hwpx의 규약과 같이 경고/오류의 존재가 종료 코드의 근거가 된다.

use serde::Serialize;

use super::parse::ParseStats;

/// 표 배출 형식별 집계.
#[derive(Debug, Serialize)]
pub struct TableCounts {
    pub pipe: usize,
    pub html: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub sections: usize,
    #[serde(rename = "paragraphCount")]
    pub paragraph_count: usize,
    pub tables: TableCounts,
    pub warnings: Vec<String>,
}

pub fn build(sections: usize, paragraph_count: usize, stats: ParseStats) -> Report {
    Report {
        sections,
        paragraph_count,
        tables: TableCounts {
            pipe: stats.pipe_tables,
            html: stats.html_tables,
        },
        warnings: stats.warnings,
    }
}
