use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

use super::dim_markdown_lines;

/// 构造带前景色的 Span
fn make_colored_span(content: &str, fg: Color) -> Span<'static> {
    Span::styled(content.to_string(), Style::default().fg(fg))
}

#[test]
fn test_dim_markdown_lines_空文本() {
    let input = Text::raw("");
    let result = dim_markdown_lines(input);
    // Text::raw("") 产生一行空 span，dim 后仍有一行
    assert_eq!(result.len(), 1);
    assert!(result[0].spans.is_empty() || result[0].spans[0].content.as_ref().is_empty());
}

#[test]
fn test_dim_markdown_lines_无前景色span设为dim() {
    let input = Text::from(vec![Line::from(vec![
        Span::raw("hello"),
        Span::raw(" world"),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    for span in &result[0].spans {
        assert_eq!(
            span.style.fg,
            Some(theme::DIM),
            "无前景色的 span 应设为 theme::DIM"
        );
    }
}

#[test]
fn test_dim_markdown_lines_有前景色span加dim修饰() {
    let input = Text::from(vec![Line::from(vec![
        make_colored_span("keyword", Color::Red),
        make_colored_span("string", Color::Green),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    // 有前景色的 span 应保留原色但加 DIM 修饰
    assert_eq!(result[0].spans[0].style.fg, Some(Color::Red));
    assert!(
        result[0].spans[0].style.add_modifier.contains(Modifier::DIM),
        "有前景色的 span 应加 DIM 修饰"
    );
    assert_eq!(result[0].spans[1].style.fg, Some(Color::Green));
    assert!(
        result[0].spans[1].style.add_modifier.contains(Modifier::DIM),
        "有前景色的 span 应加 DIM 修饰"
    );
}

#[test]
fn test_dim_markdown_lines_多行保留结构() {
    let input = Text::from(vec![
        Line::from(vec![Span::raw("line1")]),
        Line::from(vec![Span::raw("line2")]),
        Line::from(vec![Span::raw("line3")]),
    ]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 3, "应保留原始行数");
    for line in &result {
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].style.fg, Some(theme::DIM));
    }
}

#[test]
fn test_dim_markdown_lines_混合有色无色span() {
    let input = Text::from(vec![Line::from(vec![
        Span::raw("plain"),
        make_colored_span("colored", Color::Yellow),
        Span::raw("also plain"),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].spans.len(), 3);
    // 无前景色 → DIM
    assert_eq!(result[0].spans[0].style.fg, Some(theme::DIM));
    assert!(
        !result[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::DIM),
        "已设为 DIM fg 的 span 不应再加 DIM modifier"
    );
    // 有前景色 → 保留色 + DIM modifier
    assert_eq!(result[0].spans[1].style.fg, Some(Color::Yellow));
    assert!(
        result[0].spans[1].style.add_modifier.contains(Modifier::DIM)
    );
    // 无前景色 → DIM
    assert_eq!(result[0].spans[2].style.fg, Some(theme::DIM));
}

#[test]
fn test_dim_markdown_lines_内容不变() {
    let input = Text::from(vec![Line::from(vec![
        Span::raw("hello "),
        make_colored_span("world", Color::Cyan),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result[0].spans[0].content.as_ref(), "hello ");
    assert_eq!(result[0].spans[1].content.as_ref(), "world");
}
