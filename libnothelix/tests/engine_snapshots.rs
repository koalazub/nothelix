use std::collections::HashSet;

use nothelix::gallery::{self, Viewport};

#[test]
fn dimension_independent_surfaces_match_snapshots() {
    for case in gallery::fixed_cases().expect("fixed gallery cases render") {
        insta::assert_snapshot!(case.snapshot_name(), case.output);
    }
}

#[test]
fn dimension_dependent_surfaces_match_snapshots_at_every_viewport() {
    for viewport in gallery::VIEWPORTS {
        for case in gallery::viewport_cases(*viewport).expect("viewport gallery cases render") {
            insta::assert_snapshot!(case.snapshot_name(), case.output);
        }
    }
}

#[test]
fn braille_chart_fills_exactly_the_requested_viewport() {
    for viewport in gallery::VIEWPORTS {
        let chart = chart_rows(*viewport);
        assert_eq!(
            chart.len(),
            viewport.rows,
            "chart row count at {}x{}",
            viewport.cols,
            viewport.rows
        );
        for row in &chart {
            assert_eq!(
                row.chars().count(),
                viewport.cols,
                "chart row width at {}x{}",
                viewport.cols,
                viewport.rows
            );
        }
    }
}

#[test]
fn a_short_viewport_reserves_fewer_lines_than_a_tall_one() {
    let short = reserved_blank_lines(Viewport {
        cols: 120,
        rows: 12,
    });
    let tall = reserved_blank_lines(Viewport {
        cols: 120,
        rows: 40,
    });
    assert!(
        short < tall,
        "short viewport reserved {short} lines, tall reserved {tall}"
    );
}

#[test]
fn a_wide_equation_reports_a_grid_wider_than_a_narrow_viewport() {
    let narrow = grid_table(Viewport { cols: 80, rows: 24 });
    let wide = grid_table(Viewport {
        cols: 200,
        rows: 50,
    });
    assert!(
        narrow.contains("wide-alignment") && narrow.lines().any(|l| l.ends_with("  yes")),
        "narrow viewport should flag an over-wide equation:\n{narrow}"
    );
    assert!(
        wide.lines().all(|l| !l.ends_with("  yes")),
        "wide viewport should flag nothing:\n{wide}"
    );
}

#[test]
fn the_error_block_truncates_its_message_but_echoes_the_source_line_whole() {
    let long_message = case_output("error-long-message");
    assert!(
        long_message.contains("::NamedTup…"),
        "the message line should truncate at the formatter's fixed width:\n{long_message}"
    );

    let long_source = case_output("error-long-source-line");
    assert!(
        long_source.contains("21.0 22.0 23.0 24.0 25.0] * x"),
        "the source line is echoed verbatim, never clamped:\n{long_source}"
    );
    assert!(
        long_source.lines().any(|line| line.chars().count() > 120),
        "the echoed source line exceeds a 120-column terminal:\n{long_source}"
    );
}

#[test]
fn every_document_artifact_has_a_unique_name() {
    let cases = gallery::document_cases().expect("document gallery cases render");
    let mut names = HashSet::new();
    for case in &cases {
        assert!(
            names.insert(case.document_name()),
            "duplicate artifact name {}",
            case.document_name()
        );
    }
    assert_eq!(names.len(), cases.len());
}

fn case_output(name: &str) -> String {
    gallery::fixed_cases()
        .expect("fixed gallery cases render")
        .into_iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("no gallery case named {name}"))
        .output
}

fn viewport_case_output(viewport: Viewport, name: &str) -> String {
    gallery::viewport_cases(viewport)
        .expect("viewport gallery cases render")
        .into_iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("no gallery case named {name}"))
        .output
}

fn chart_rows(viewport: Viewport) -> Vec<String> {
    viewport_case_output(viewport, "braille-chart")
        .lines()
        .skip(2)
        .map(str::to_string)
        .collect()
}

fn grid_table(viewport: Viewport) -> String {
    viewport_case_output(viewport, "math-image-grid")
}

fn reserved_blank_lines(viewport: Viewport) -> usize {
    viewport_case_output(viewport, "math-reserve")
        .lines()
        .filter(|line| line.trim_end() == "#")
        .count()
}
