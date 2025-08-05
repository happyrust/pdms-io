# PDMS Raphtory GraphQL 查询示例

## 1. 基础图信息查询

### 查询图的基本统计信息
```graphql
query GraphStats {
  graph(path: "pdms_complete_pe_data") {
    name
    nodeCount
    edgeCount
    earliestTime
    latestTime
  }
}
```

## 2. 节点查询

### 查询所有节点（带分页）
```graphql
query AllNodes {
  graph(path: "pdms_complete_pe_data") {
    nodes(limit: 10, offset: 0) {
      name
      nodeType
      earliestTime
      latestTime
      properties {
        refno
        sesno
        dbnum
        element_type
        operation_type
        owner_refno
        deleted
      }
    }
  }
}
```

### 查询特定节点的详细信息
```graphql
query NodeDetails($nodeName: String!) {
  graph(path: "pdms_complete_pe_data") {
    node(name: $nodeName) {
      name
      nodeType
      earliestTime
      latestTime
      history
      properties {
        refno
        sesno
        dbnum
        element_type
        operation_type
        owner_refno
        deleted
        # PDMS 特定属性
        NAME
        XLEN
        YLEN
        ZLEN
        POS_WRT_WORLD
        ORI_WRT_WORLD
      }
    }
  }
}

# 变量示例：
# {
#   "nodeName": "PE_17496_496553"
# }
```

## 3. 关系查询

### 查询节点的所有子元素（Children）
```graphql
query NodeChildren($nodeName: String!) {
  graph(path: "pdms_complete_pe_data") {
    node(name: $nodeName) {
      name
      properties {
        element_type
        NAME
      }
      outEdges {
        dst {
          name
          properties {
            refno
            element_type
            NAME
            deleted
          }
        }
        properties {
          relationship
          sesno
        }
        earliestTime
        latestTime
      }
    }
  }
}
```

### 查询节点的父元素（Owner）
```graphql
query NodeOwner($nodeName: String!) {
  graph(path: "pdms_complete_pe_data") {
    node(name: $nodeName) {
      name
      properties {
        element_type
        owner_refno
      }
      inEdges {
        src {
          name
          properties {
            refno
            element_type
            NAME
          }
        }
        properties {
          relationship
        }
      }
    }
  }
}
```

### 递归查询所有后代元素
```graphql
query NodeDescendants($nodeName: String!, $hops: Int!) {
  graph(path: "pdms_complete_pe_data") {
    node(name: $nodeName) {
      name
      properties {
        element_type
        NAME
      }
      neighbours(hops: $hops, direction: OUT) {
        name
        properties {
          refno
          element_type
          NAME
          deleted
        }
      }
    }
  }
}

# 变量示例：
# {
#   "nodeName": "PE_17496_496514",
#   "hops": 3
# }
```

## 4. 时间相关查询

### 查询特定时间点的图状态
```graphql
query GraphAtTime($timestamp: Int!) {
  graph(path: "pdms_complete_pe_data") {
    at(time: $timestamp) {
      nodeCount
      edgeCount
      nodes(limit: 10) {
        name
        properties {
          refno
          element_type
          operation_type
          deleted
        }
      }
    }
  }
}

# 变量示例（使用会话号作为时间戳）：
# {
#   "timestamp": 897
# }
```

### 查询节点的历史变化
```graphql
query NodeHistory($nodeName: String!) {
  graph(path: "pdms_complete_pe_data") {
    node(name: $nodeName) {
      name
      history
      properties {
        element_type
      }
      # 查看属性的历史值
      propertiesAt(time: 896) {
        operation_type
        deleted
      }
    }
  }
}
```

### 查询时间范围内的变化
```graphql
query ChangesInTimeRange($startTime: Int!, $endTime: Int!) {
  graph(path: "pdms_complete_pe_data") {
    window(start: $startTime, end: $endTime) {
      nodes {
        name
        properties {
          refno
          element_type
          operation_type
          sesno
        }
      }
      edges {
        src {
          name
        }
        dst {
          name
        }
        properties {
          relationship
        }
      }
    }
  }
}
```

## 5. 高级查询

### 查询所有已删除的元素
```graphql
query DeletedElements {
  graph(path: "pdms_complete_pe_data") {
    nodes(limit: 100) {
      name
      properties {
        refno
        element_type
        deleted
        sesno
      }
    }
  }
}
```

### 查询特定类型的所有元素
```graphql
query ElementsByType($elementType: String!) {
  graph(path: "pdms_complete_pe_data") {
    nodes {
      name
      properties {
        refno
        element_type
        NAME
        owner_refno
      }
    }
  }
}

# 注意：需要在客户端过滤 element_type
```

### 查询修改操作的详细信息
```graphql
query ModifiedElements {
  graph(path: "pdms_complete_pe_data") {
    nodes {
      name
      properties {
        refno
        element_type
        operation_type
        added_attrs_count
        deleted_attrs_count
        modified_attrs_count
        sesno
      }
      history
    }
  }
}
```

### 查询元素的完整层级路径
```graphql
query ElementPath($nodeName: String!) {
  graph(path: "pdms_complete_pe_data") {
    node(name: $nodeName) {
      name
      properties {
        element_type
        NAME
      }
      # 向上查找所有祖先（最多5层）
      neighbours(hops: 5, direction: IN) {
        name
        properties {
          element_type
          NAME
        }
      }
    }
  }
}
```

### 批量查询多个节点
```graphql
query MultipleNodes($nodeNames: [String!]!) {
  graph(path: "pdms_complete_pe_data") {
    nodes {
      name
      properties {
        refno
        element_type
        NAME
        owner_refno
        deleted
      }
    }
  }
}

# 注意：需要在客户端过滤指定的节点名称
```

## 6. 性能优化查询

### 仅获取节点名称列表
```graphql
query NodeNamesList {
  graph(path: "pdms_complete_pe_data") {
    nodes(limit: 1000) {
      name
    }
  }
}
```

### 获取边的统计信息
```graphql
query EdgeStatistics {
  graph(path: "pdms_complete_pe_data") {
    edges(limit: 10) {
      src {
        name
        properties {
          element_type
        }
      }
      dst {
        name
        properties {
          element_type
        }
      }
      earliestTime
      latestTime
      deletions
    }
  }
}
```

## 7. 实用查询模板

### 查找孤儿节点（没有父节点的元素）
```graphql
query OrphanNodes {
  graph(path: "pdms_complete_pe_data") {
    nodes {
      name
      properties {
        refno
        element_type
        owner_refno
      }
      inDegree
    }
  }
}

# 在客户端过滤 inDegree == 0 的节点
```

### 查询元素的兄弟节点
```graphql
query Siblings($nodeName: String!) {
  graph(path: "pdms_complete_pe_data") {
    node(name: $nodeName) {
      properties {
        owner_refno
      }
      # 先找到父节点，再找其所有子节点
      inEdges {
        src {
          name
          outEdges {
            dst {
              name
              properties {
                refno
                element_type
                NAME
              }
            }
          }
        }
      }
    }
  }
}
```

## 使用说明

1. **变量使用**：带有 `$` 符号的是 GraphQL 变量，需要在查询时提供具体值

2. **时间戳**：在我们的实现中，时间戳使用的是：
   - Unix 毫秒时间戳（通过 get_sesno_datetime 转换）
   - 如果转换失败，会使用会话号作为时间戳

3. **节点命名**：所有节点使用 `PE_` 前缀，如 `PE_17496_496553`

4. **关系类型**：主要的关系是 "owns"（拥有关系）

5. **注意事项**：
   - Raphtory GraphQL 可能不支持所有标准 GraphQL 特性
   - 某些过滤操作需要在客户端完成
   - 大量数据查询时注意使用分页（limit 和 offset）

6. **调试技巧**：
   - 先查询少量数据了解结构
   - 使用 history 字段查看时间点
   - 检查 deletions 字段了解边的删除情况