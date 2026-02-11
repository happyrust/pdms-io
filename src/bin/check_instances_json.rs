use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::env;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let json_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "D:/work/plant-code/gen_model-dev/.worktrees/tubi-cache-order/output/AvevaMarineSample/instances/instances_7997.json".to_string());
    let owner_refno = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "24381_145018".to_string());

    let content =
        fs::read_to_string(&json_path).with_context(|| format!("读取失败: {}", json_path))?;
    let root: Value =
        serde_json::from_str(&content).with_context(|| format!("解析 JSON 失败: {}", json_path))?;

    let groups = root
        .get("groups")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("JSON 中缺少 groups 数组"))?;

    let group = groups
        .iter()
        .find(|g| g.get("owner_refno").and_then(|v| v.as_str()) == Some(owner_refno.as_str()))
        .ok_or_else(|| anyhow!("未找到 owner_refno={}", owner_refno))?;

    let children_count = group
        .get("children")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let tubings = group
        .get("tubings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let tubings_count = tubings.len();

    let mut orders: Vec<i64> = tubings
        .iter()
        .filter_map(|t| t.get("order").and_then(|v| v.as_i64()))
        .collect();
    orders.sort();
    let has_dup = orders.windows(2).any(|w| w[0] == w[1]);
    let strict_inc = !orders.is_empty()
        && !has_dup
        && orders[0] == 0
        && orders[orders.len() - 1] == (orders.len() as i64 - 1)
        && orders.windows(2).all(|w| w[1] == w[0] + 1);

    println!("instances_json: {}", json_path);
    println!("owner_refno: {}", owner_refno);
    println!("children_count: {}", children_count);
    println!("tubings_count: {}", tubings_count);
    println!("order_strict_inc: {}", strict_inc);

    Ok(())
}
