fn main() {
    let src = std::fs::read_to_string("tests/wasp/inputs/hit/recovery_missing_block_name.i").unwrap();
    let d = ruhit::parse_with_options(&src, ruhit::ParseOptions::wasp());
    println!("items: {:#?}", d.items.iter().map(|i| match i { ruhit::Item::Block(b)=>format!("block {:?}",b.name), ruhit::Item::Parameter(p)=>format!("param {}",p.key), ruhit::Item::Comment(c)=>format!("comment {}",c.text)}).collect::<Vec<_>>());
    println!("diags: {:#?}", d.diagnostics.iter().map(|x|(x.span.start.line,x.span.start.column,x.message.clone())).collect::<Vec<_>>());
}
