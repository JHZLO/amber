// 노트 본문을 글 덩어리와 그림으로 가른다. 그림은 ```svg / ```mermaid 펜스(지금 앱의 mdSvg, mdMermaid 와 같은 표기),
// 나머지 펜스(코드)는 글 덩어리에 그대로 남겨 마크다운 렌더러가 그린다.

pub enum Segment {
    Markdown(String),
    Svg(String),
    Mermaid(String),
}

pub fn split(md: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut lines = md.lines();
    while let Some(line) = lines.next() {
        let lang = line.trim_start().strip_prefix("```").map(str::trim);
        if let Some(lang @ ("svg" | "mermaid")) = lang {
            let mut body = String::new();
            for inner in lines.by_ref() {
                if inner.trim() == "```" {
                    break;
                }
                body.push_str(inner);
                body.push('\n');
            }
            if !text.trim().is_empty() {
                out.push(Segment::Markdown(std::mem::take(&mut text)));
            }
            text.clear();
            out.push(if lang == "svg" {
                Segment::Svg(body)
            } else {
                Segment::Mermaid(body)
            });
            continue;
        }
        text.push_str(line);
        text.push('\n');
    }
    if !text.trim().is_empty() {
        out.push(Segment::Markdown(text));
    }
    out
}
