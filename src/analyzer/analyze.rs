
pub fn Analyze( path: Option<String> ) -> anyhow::Result<()>
{
    match path {
        Some(p) => println!("Path: {}", p),
        None => println!("No path provided"),
    }

    Ok(())
}