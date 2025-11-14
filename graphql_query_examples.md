# Raphtory GraphQL 查询示例

这个文档包含了查询 PDMS 数据的各种 GraphQL 查询示例。

## 基础查询

### 1. 查询单个节点的所有属性

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      name
      properties {
        values {
          key
          value
        }
      }
    }
  }
}
```

### 2. 查询特定属性

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      name
      properties {
        refno: get(key: "refno") { value }
        elementType: get(key: "element_type") { value }
        dbnum: get(key: "dbnum") { value }
        sesno: get(key: "sesno") { value }
        deleted: get(key: "deleted") { value }
        ownerRefno: get(key: "owner_refno") { value }
      }
    }
  }
}
```

### 3. 查询所有节点（带分页）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        properties {
          values {
            key
            value
          }
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
}
```

## 关系查询

### 4. 查询节点的所有子元素（通过出边）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      name
      outEdges {
        list {
          dst {
            name
            properties {
              values {
                key
                value
              }
            }
          }
        }
      }
    }
  }
}
```

### 5. 查询节点的父元素（通过入边）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496567") {
      name
      inEdges {
        list {
          src {
            name
            properties {
              values {
                key
                value
              }
            }
          }
        }
      }
    }
  }
}
```

### 6. 深度查询 - 两层子元素

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_272497") {
      name
      properties {
        elementType: get(key: "element_type") { value }
      }
      outEdges {
        list {
          dst {
            name
            properties {
              elementType: get(key: "element_type") { value }
            }
            outEdges {
              list {
                dst {
                  name
                  properties {
                    elementType: get(key: "element_type") { value }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

## 过滤查询

### 7. 按类型过滤节点（需要客户端处理）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        properties {
          values {
            key
            value
          }
        }
      }
    }
  }
}
```

使用 JavaScript 客户端过滤：
```javascript
const pfitNodes = response.data.graph.nodes.list.filter(node => {
  const props = node.properties.values.reduce((acc, {key, value}) => {
    acc[key] = value;
    return acc;
  }, {});
  return props.element_type === 'PFIT';
});
```

### 8. 查询特定数据库编号的节点

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        properties {
          dbnum: get(key: "dbnum") { value }
          elementType: get(key: "element_type") { value }
          refno: get(key: "refno") { value }
        }
      }
    }
  }
}
```

## 时间相关查询

### 9. 查询节点在特定时间点的状态

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      at(time: 897) {
        name
        properties {
          values {
            key
            value
          }
        }
      }
    }
  }
}
```

### 10. 查询节点的历史变化

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      history {
        time
        properties {
          values {
            key
            value
          }
        }
      }
    }
  }
}
```

## 复杂查询示例

### 11. 查询 FLOOR 类型节点及其所有 PFIT 子元素

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        properties {
          elementType: get(key: "element_type") { value }
          refno: get(key: "refno") { value }
        }
        outEdges {
          list {
            dst {
              name
              properties {
                elementType: get(key: "element_type") { value }
                refno: get(key: "refno") { value }
              }
            }
          }
        }
      }
    }
  }
}
```

### 12. 统计查询 - 获取节点数量和边数量

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      count
    }
    edges {
      count
    }
  }
}
```

### 13. 批量查询多个节点

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node1: node(name: "PE_17496_496514") {
      properties {
        refno: get(key: "refno") { value }
        elementType: get(key: "element_type") { value }
      }
    }
    node2: node(name: "PE_17496_496567") {
      properties {
        refno: get(key: "refno") { value }
        elementType: get(key: "element_type") { value }
      }
    }
    node3: node(name: "PE_17496_272497") {
      properties {
        refno: get(key: "refno") { value }
        elementType: get(key: "element_type") { value }
      }
    }
  }
}
```

### 14. 查询节点的完整层次结构（递归查询）

```graphql
fragment NodeInfo on Node {
  name
  properties {
    refno: get(key: "refno") { value }
    elementType: get(key: "element_type") { value }
    name: get(key: "NAME") { value }
  }
}

{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_272497") {
      ...NodeInfo
      outEdges {
        list {
          dst {
            ...NodeInfo
            outEdges {
              list {
                dst {
                  ...NodeInfo
                  outEdges {
                    list {
                      dst {
                        ...NodeInfo
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

### 15. 查询带有特定属性的节点

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        properties {
          hasName: get(key: "NAME") { value }
          hasOwner: get(key: "owner_refno") { value }
          deleted: get(key: "deleted") { value }
          values {
            key
            value
          }
        }
      }
    }
  }
}
```

## 实用查询模板

### 16. 获取元素的完整信息（包括位置、方向等）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      name
      properties {
        # 基本信息
        refno: get(key: "refno") { value }
        elementType: get(key: "element_type") { value }
        dbnum: get(key: "dbnum") { value }
        sesno: get(key: "sesno") { value }
        
        # 位置和方向
        position: get(key: "POS") { value }
        orientation: get(key: "ORI") { value }
        
        # 层级信息
        level: get(key: "LEVE") { value }
        
        # 其他属性
        name: get(key: "NAME") { value }
        owner: get(key: "owner_refno") { value }
        deleted: get(key: "deleted") { value }
      }
    }
  }
}
```

### 17. 查询并导出为 JSON 格式

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    export: nodes {
      list {
        name
        properties {
          values {
            key
            value
          }
        }
        edges: outEdges {
          list {
            target: dst {
              name
            }
          }
        }
      }
    }
  }
}
```

## 性能优化查询

### 18. 只获取节点名称列表

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
      }
    }
  }
}
```

### 19. 获取特定属性的节点（最小化数据传输）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        properties {
          elementType: get(key: "element_type") { value }
          refno: get(key: "refno") { value }
        }
      }
    }
  }
}
```

### 20. 使用别名进行批量查询

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    floors: nodes {
      list {
        name
        properties {
          type: get(key: "element_type") { value }
        }
      }
    }
    
    specificNode: node(name: "PE_17496_496514") {
      children: outEdges {
        count
      }
    }
  }
}
```

## 使用 cURL 的命令行示例

### 基本查询
```bash
curl -X POST http://localhost:1737/ \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ graph(path: \"pdms_complete_pe_data\") { node(name: \"PE_17496_496514\") { properties { values { key value } } } } }"
  }'
```

### 格式化输出
```bash
curl -X POST http://localhost:1737/ \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ graph(path: \"pdms_complete_pe_data\") { node(name: \"PE_17496_496514\") { properties { values { key value } } } } }"
  }' | jq .
```

### 提取特定值
```bash
curl -X POST http://localhost:1737/ \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ graph(path: \"pdms_complete_pe_data\") { node(name: \"PE_17496_496514\") { properties { refno: get(key: \"refno\") { value } } } } }"
  }' | jq -r '.data.graph.node.properties.refno.value'
```

## 时间查询 (Temporal Queries)

### 21. 查询特定时间点的元素状态

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      at(time: 1734413472000) {
        exists
        properties {
          values {
            key
            value
          }
        }
      }
    }
  }
}
```

### 22. 查询特定时间点的元素及其子元素

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      at(time: 1734413472000) {
        exists
        properties {
          refno: get(key: "refno") { value }
          elementType: get(key: "element_type") { value }
          sesno: get(key: "sesno") { value }
        }
        outEdges {
          list {
            dst {
              name
              properties {
                refno: get(key: "refno") { value }
                elementType: get(key: "element_type") { value }
                deleted: get(key: "deleted") { value }
              }
            }
          }
        }
      }
    }
  }
}
```

### 23. 查询元素的完整历史

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      history {
        time
        properties {
          values {
            key
            value
          }
        }
      }
    }
  }
}
```

### 24. 查询元素的历史变化（带属性过滤）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496567") {
      history {
        time
        properties {
          refno: get(key: "refno") { value }
          sesno: get(key: "sesno") { value }
          operationType: get(key: "operation_type") { value }
          elementType: get(key: "element_type") { value }
          deleted: get(key: "deleted") { value }
        }
      }
    }
  }
}
```

### 25. 查询多个时间点的状态对比

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      time1: at(time: 1734413472000) {
        properties {
          name: get(key: "NAME") { value }
          position: get(key: "POS") { value }
          owner: get(key: "owner_refno") { value }
        }
      }
      time2: at(time: 1734417072000) {
        properties {
          name: get(key: "NAME") { value }
          position: get(key: "POS") { value }
          owner: get(key: "owner_refno") { value }
        }
      }
    }
  }
}
```

### 26. 查询特定时间点的完整层次结构

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_272497") {
      at(time: 1734413472000) {
        name
        properties {
          elementType: get(key: "element_type") { value }
        }
        level1: outEdges {
          list {
            dst {
              name
              properties {
                elementType: get(key: "element_type") { value }
              }
              level2: outEdges {
                list {
                  dst {
                    name
                    properties {
                      elementType: get(key: "element_type") { value }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

### 27. 查询边的历史（如果支持）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    edges {
      list {
        src {
          name
        }
        dst {
          name
        }
        history {
          time
          properties {
            relationship: get(key: "relationship") { value }
          }
        }
      }
    }
  }
}
```

### 28. 时间范围查询（需要客户端过滤）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        history {
          time
          properties {
            refno: get(key: "refno") { value }
            operationType: get(key: "operation_type") { value }
            elementType: get(key: "element_type") { value }
          }
        }
      }
    }
  }
}
```

客户端过滤示例：
```javascript
// 过滤特定时间范围内的修改
const startTime = 1734413472000;
const endTime = 1734417072000;

const modifications = response.data.graph.nodes.list.flatMap(node => 
  node.history
    .filter(entry => entry.time >= startTime && entry.time <= endTime)
    .map(entry => ({
      nodeName: node.name,
      time: entry.time,
      operationType: entry.properties.operationType.value,
      elementType: entry.properties.elementType.value
    }))
);
```

### 29. 查询元素创建和最后修改时间

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      name
      history {
        time
        properties {
          operationType: get(key: "operation_type") { value }
        }
      }
    }
  }
}
```

### 30. 时间点快照查询（完整图状态）

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    snapshot: at(time: 1734413472000) {
      nodes {
        list {
          name
          properties {
            refno: get(key: "refno") { value }
            elementType: get(key: "element_type") { value }
            deleted: get(key: "deleted") { value }
          }
        }
      }
      edges {
        list {
          src { name }
          dst { name }
          properties {
            relationship: get(key: "relationship") { value }
          }
        }
      }
    }
  }
}
```

## 基于 sesno 的查询 (Session-based Queries)

### 31. 将 sesno 转换为时间戳

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        history {
          time
          properties {
            sesno: get(key: "sesno") { value }
          }
        }
      }
    }
  }
}
```

客户端处理示例（查找 sesno 897 对应的时间戳）：
```javascript
const targetSesno = 897;
let timestamp = null;

for (const node of response.data.graph.nodes.list) {
  for (const entry of node.history) {
    if (entry.properties.sesno.value === targetSesno) {
      timestamp = entry.time;
      break;
    }
  }
  if (timestamp) break;
}

console.log(`sesno ${targetSesno} 对应的时间戳: ${timestamp}`);
```

### 32. 查询指定 sesno 的元素状态

```graphql
# 首先获取 sesno 对应的时间戳，然后使用时间查询
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      at(time: 1734413472000) {  # 使用 sesno 897 对应的时间戳
        exists
        properties {
          values {
            key
            value
          }
        }
      }
    }
  }
}
```

### 33. 查询指定 sesno 的层次结构

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_272497") {
      at(time: 1734413472000) {  # 使用 sesno 897 对应的时间戳
        name
        properties {
          refno: get(key: "refno") { value }
          elementType: get(key: "element_type") { value }
          sesno: get(key: "sesno") { value }
        }
        level1: outEdges {
          list {
            dst {
              name
              properties {
                refno: get(key: "refno") { value }
                elementType: get(key: "element_type") { value }
                sesno: get(key: "sesno") { value }
              }
              level2: outEdges {
                list {
                  dst {
                    name
                    properties {
                      refno: get(key: "refno") { value }
                      elementType: get(key: "element_type") { value }
                      sesno: get(key: "sesno") { value }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

### 34. 查询指定 sesno 的所有元素变化

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        history {
          time
          properties {
            values {
              key
              value
            }
          }
        }
      }
    }
  }
}
```

客户端过滤 sesno 897 的变化：
```javascript
const targetSesno = 897;
const changes = [];

for (const node of response.data.graph.nodes.list) {
  for (const entry of node.history) {
    const props = entry.properties.values.reduce((acc, {key, value}) => {
      acc[key] = value;
      return acc;
    }, {});
    
    if (props.sesno === targetSesno) {
      changes.push({
        nodeName: node.name,
        time: entry.time,
        refno: props.refno,
        operationType: props.operation_type,
        elementType: props.element_type
      });
    }
  }
}

console.log(`sesno ${targetSesno} 中有 ${changes.length} 个元素变化`);
```

### 35. 获取指定 sesno 的模型快照

```graphql
# 查询所有在指定时间点存在的节点
{
  graph(path: "pdms_complete_pe_data") {
    at(time: 1734413472000) {  # 使用 sesno 897 对应的时间戳
      nodes {
        list {
          name
          properties {
            refno: get(key: "refno") { value }
            elementType: get(key: "element_type") { value }
            sesno: get(key: "sesno") { value }
            deleted: get(key: "deleted") { value }
          }
        }
      }
    }
  }
}
```

### 36. sesno 版本对比查询

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496514") {
      sesno896: at(time: 1734413470000) {  # sesno 896 的时间戳
        properties {
          name: get(key: "NAME") { value }
          position: get(key: "POS") { value }
          elementType: get(key: "element_type") { value }
        }
      }
      sesno897: at(time: 1734413472000) {  # sesno 897 的时间戳
        properties {
          name: get(key: "NAME") { value }
          position: get(key: "POS") { value }
          elementType: get(key: "element_type") { value }
        }
      }
    }
  }
}
```

### 37. 查询元素在哪个 sesno 被创建

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    node(name: "PE_17496_496567") {
      history {
        time
        properties {
          sesno: get(key: "sesno") { value }
          operationType: get(key: "operation_type") { value }
        }
      }
    }
  }
}
```

客户端处理：
```javascript
const history = response.data.graph.node.history;

// 找出创建操作
const creation = history.find(entry => 
  entry.properties.operationType.value === "ADD"
);

if (creation) {
  console.log(`元素创建于 sesno: ${creation.properties.sesno.value}`);
  console.log(`创建时间: ${new Date(creation.time).toISOString()}`);
}

// 找出最后修改
const lastModification = history[history.length - 1];
console.log(`最后修改于 sesno: ${lastModification.properties.sesno.value}`);
```

### 38. 查询特定 sesno 范围内的所有变化

```graphql
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        history {
          time
          properties {
            sesno: get(key: "sesno") { value }
            operationType: get(key: "operation_type") { value }
            elementType: get(key: "element_type") { value }
            refno: get(key: "refno") { value }
          }
        }
      }
    }
  }
}
```

客户端过滤 sesno 895 到 897 的变化：
```javascript
const startSesno = 895;
const endSesno = 897;
const changesInRange = [];

for (const node of response.data.graph.nodes.list) {
  for (const entry of node.history) {
    const sesno = entry.properties.sesno.value;
    if (sesno >= startSesno && sesno <= endSesno) {
      changesInRange.push({
        sesno: sesno,
        time: entry.time,
        refno: entry.properties.refno.value,
        operationType: entry.properties.operationType.value,
        elementType: entry.properties.elementType.value
      });
    }
  }
}

// 按 sesno 分组
const bySesno = changesInRange.reduce((acc, change) => {
  if (!acc[change.sesno]) acc[change.sesno] = [];
  acc[change.sesno].push(change);
  return acc;
}, {});

for (const [sesno, changes] of Object.entries(bySesno)) {
  console.log(`sesno ${sesno}: ${changes.length} 个变化`);
}
```

### 39. 查询根节点在指定 sesno 的状态

```graphql
# 查询所有没有入边的节点（根节点）
{
  graph(path: "pdms_complete_pe_data") {
    nodes {
      list {
        name
        at(time: 1734413472000) {  # 使用 sesno 897 对应的时间戳
          exists
          inEdges {
            count
          }
          properties {
            refno: get(key: "refno") { value }
            elementType: get(key: "element_type") { value }
            deleted: get(key: "deleted") { value }
          }
        }
      }
    }
  }
}
```

客户端处理找出根节点：
```javascript
const rootNodes = [];

for (const node of response.data.graph.nodes.list) {
  const at = node.at;
  if (at.exists && at.inEdges.count === 0 && !at.properties.deleted.value) {
    rootNodes.push({
      name: node.name,
      refno: at.properties.refno.value,
      elementType: at.properties.elementType.value
    });
  }
}

console.log(`在指定时间点找到 ${rootNodes.length} 个根节点`);
```

### 40. sesno 相关的综合查询示例

```graphql
# 使用片段(fragment)简化重复的属性查询
fragment ElementProps on Node {
  properties {
    refno: get(key: "refno") { value }
    elementType: get(key: "element_type") { value }
    name: get(key: "NAME") { value }
    sesno: get(key: "sesno") { value }
    deleted: get(key: "deleted") { value }
  }
}

{
  graph(path: "pdms_complete_pe_data") {
    # 1. 获取特定元素的历史
    elementHistory: node(name: "PE_17496_496514") {
      history {
        time
        properties {
          sesno: get(key: "sesno") { value }
          operationType: get(key: "operation_type") { value }
        }
      }
    }
    
    # 2. 获取特定时间点的状态
    elementAtTime: node(name: "PE_17496_496514") {
      at(time: 1734413472000) {
        ...ElementProps
      }
    }
    
    # 3. 获取统计信息
    stats: at(time: 1734413472000) {
      nodeCount: nodes { count }
      edgeCount: edges { count }
    }
  }
}
```

## 注意事项

1. **属性访问**：所有自定义属性都需要通过 `get(key: "xxx")` 或 `values` 数组访问
2. **节点命名**：所有节点名称都是 `PE_` 前缀加上 refno
3. **时间戳**：使用毫秒级时间戳进行时间查询，对应于 sesno 的转换时间
4. **关系类型**：主要的关系是 "owns"（所有者关系）
5. **性能**：对于大量数据，使用分页和限制返回字段来优化性能
6. **时间查询支持**：某些时间查询功能（如 `at()` 和 `history`）需要 Raphtory 的相应版本支持
7. **客户端过滤**：对于复杂的时间范围查询，可能需要在客户端进行额外的过滤处理
8. **sesno 查询**：基于 sesno 的查询通常需要先转换为时间戳，然后使用时间查询功能
9. **sesno 到时间戳映射**：建议在客户端缓存 sesno 到时间戳的映射以提高性能