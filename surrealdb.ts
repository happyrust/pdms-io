import { Surreal } from 'surrealdb.js';

// SurrealDB连接配置
const SURREAL_URL = process.env.NEXT_PUBLIC_SURREAL_URL || 'http://127.0.0.1:8009/rpc';
const NAMESPACE = process.env.NEXT_PUBLIC_SURREAL_NS || '1516';
const DATABASE = process.env.NEXT_PUBLIC_SURREAL_DB || 'AvevaMarineSample';
// 提供有更高权限的用户凭据
const USERNAME = process.env.NEXT_PUBLIC_SURREAL_USER || 'root';
const PASSWORD = process.env.NEXT_PUBLIC_SURREAL_PASS || 'root';

// 创建SurrealDB连接单例
let db: Surreal | null = null;
let dbConnected = false;

/**
 * 检查数据库连接状态
 * @returns 数据库是否已连接
 */
export async function checkDatabaseConnection() {
  if (!db) {
    await initSurrealDB();
  }
  
  if (!db) {
    return false;
  }
  
  try {
    // 执行一个简单查询来测试连接
    const result = await db.query('RETURN 1');
    dbConnected = true;
    return true;
  } catch (error) {
    console.error('数据库连接测试失败:', error);
    dbConnected = false;
    return false;
  }
}

/**
 * 获取数据库连接状态
 * @returns 数据库是否已连接
 */
export function isDatabaseConnected() {
  return dbConnected;
}

/**
 * 初始化SurrealDB连接
 */
export async function initSurrealDB() {
  if (!db) {
    db = new Surreal();
    
    try {
      // 连接到SurrealDB
      await db.connect(SURREAL_URL);
      
      // 使用指定的namespace和database
      await db.use({
        namespace: NAMESPACE,
        database: DATABASE
      });
      
      // 登录账号
      await db.signin({
        username: USERNAME,
        password: PASSWORD,
      });
      
      console.log('SurrealDB连接成功');
      dbConnected = true;
    } catch (err) {
      console.error('SurrealDB连接失败:', err);
      db = null;
      dbConnected = false;
    }
  }
  
  return db;
}

/**
 * 获取SurrealDB连接实例
 */
export async function getDB() {
  if (!db) {
    await initSurrealDB();
  }
  return db;
}

/**
 * 获取元素变更记录
 * @param limit 限制返回的记录数量
 * @param offset 偏移量
 */
export async function getElementChanges(limit = 20, offset = 0) {
  const db = await getDB();
  if (!db) return [];
  
  try {
    const result = await db.query(`
      SELECT 
        id,
        refno,
        operation_type,
        entity_type,
        timestamp,
        sesno,
        session_id.project as project,
        session_id.sesno as session_number,
        details
      FROM element_changes
      ORDER BY timestamp DESC
      LIMIT $limit
      START $offset
    `, {
      limit,
      offset,
    });
    
    // 确保返回的是正确的类型
    return Array.isArray(result) ? (result as unknown as any[]) : [];
  } catch (err) {
    console.error('获取元素变更记录失败:', err);
    return [];
  }
}

/**
 * 获取最近一段时间内的变更统计
 * @param days 天数
 */
export async function getRecentChangesStats(days = 7) {
  const db = await getDB();
  if (!db) return [];
  
  try {
    // 首先获取每天的总数和日期分组
    // 恢复使用 time::group 并添加 timestamp IS NOT NULL
    const dailyAggregatesResult = await db.query(` 
      SELECT 
        time::group(timestamp, '1d') AS date_group,
        count() AS total_for_day
      FROM element_changes
      WHERE timestamp IS NOT NULL AND timestamp > time::now() - ${days}d
      GROUP BY date_group
      ORDER BY date_group
    `);

    // 过滤掉 date_group 为 null 或 total_for_day 无效的记录
    const validAggregates = (dailyAggregatesResult as any[])
      .filter(item => item.date_group != null && typeof item.total_for_day === 'number');

    if (validAggregates.length === 0) {
      return []; // 如果没有有效数据，则返回空数组
    }
    
    const getCountFromResult = (queryResult: any): number => {
      if (Array.isArray(queryResult) && 
          queryResult.length > 0 && 
          queryResult[0] && 
          typeof queryResult[0] === 'object' && 
          'count' in queryResult[0] &&
          typeof queryResult[0].count === 'number') {
        return queryResult[0].count;
      }
      return 0;
    };

    // 对每个有效的日期分组获取详细统计
    const statsPromises = validAggregates.map(async (aggItem) => {
      const currentDateString = String(aggItem.date_group); // 确保是字符串，如 '2023-10-26T00:00:00Z'
      const totalForThisDate = aggItem.total_for_day;

      const addResult = await db.query(`
        SELECT count() AS count
        FROM element_changes
        WHERE timestamp >= <datetime>$dateParam AND timestamp < (<datetime>$dateParam + 1d)
        AND operation_type = '新增'
      `, { dateParam: currentDateString });

      const modifyResult = await db.query(`
        SELECT count() AS count
        FROM element_changes
        WHERE timestamp >= <datetime>$dateParam AND timestamp < (<datetime>$dateParam + 1d)
        AND operation_type = '修改'
      `, { dateParam: currentDateString });

      const deleteResult = await db.query(`
        SELECT count() AS count
        FROM element_changes
        WHERE timestamp >= <datetime>$dateParam AND timestamp < (<datetime>$dateParam + 1d)
        AND operation_type = '删除'
      `, { dateParam: currentDateString });
      
      return {
        date: currentDateString, // 将在最后格式化
        total: totalForThisDate,
        add_count: getCountFromResult(addResult),
        modify_count: getCountFromResult(modifyResult),
        delete_count: getCountFromResult(deleteResult)
      };
    });

    const stats = await Promise.all(statsPromises);
    
    // 确保返回的日期是 YYYY-MM-DD 格式
    return stats.map(s => ({ 
      ...s, 
      // s.date 此时应该是 'YYYY-MM-DDTHH:mm:ssZ' 格式的字符串
      date: String(s.date).split('T')[0]
    }));
  } catch (err) {
    console.error('获取变更统计失败:', err);
    return [];
  }
}

/**
 * 根据参考号获取元素的所有变更历史
 * @param refno 参考号
 */
export async function getElementHistory(refno: string) {
  const db = await getDB();
  if (!db) return [];
  
  try {
    const result = await db.query(`
      SELECT 
        id,
        refno,
        operation_type,
        entity_type,
        timestamp,
        sesno,
        session_id,
        details
      FROM element_changes
      WHERE refno = $refno
      ORDER BY timestamp DESC
    `, {
      refno,
    });
    
    return Array.isArray(result) ? result : [];
  } catch (err) {
    console.error('获取元素历史记录失败:', err);
    return [];
  }
}

/**
 * 获取变更总览数据
 */
export async function getChangesOverview() {
  const db = await getDB();
  if (!db) return null;
  
  try {
    // 获取今日日期字符串
    const todayString = new Date().toISOString().split('T')[0];
    
    // 获取今日变更统计
    const todayAddResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$today AND timestamp < (<datetime>$today + 1d)
      AND operation_type = '新增'
    `, { today: todayString });
    
    const todayModifyResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$today AND timestamp < (<datetime>$today + 1d)
      AND operation_type = '修改'
    `, { today: todayString });
    
    const todayDeleteResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$today AND timestamp < (<datetime>$today + 1d)
      AND operation_type = '删除'
    `, { today: todayString });
    
    const todayTotalResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$today AND timestamp < (<datetime>$today + 1d)
    `, { today: todayString });
    
    // 获取昨日变更统计
    const yesterdayDate = new Date();
    yesterdayDate.setDate(yesterdayDate.getDate() - 1);
    const yesterdayString = yesterdayDate.toISOString().split('T')[0];
    
    const yesterdayAddResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$yesterday AND timestamp < (<datetime>$yesterday + 1d)
      AND operation_type = '新增'
    `, { yesterday: yesterdayString });
    
    const yesterdayModifyResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$yesterday AND timestamp < (<datetime>$yesterday + 1d)
      AND operation_type = '修改'
    `, { yesterday: yesterdayString });
    
    const yesterdayDeleteResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$yesterday AND timestamp < (<datetime>$yesterday + 1d)
      AND operation_type = '删除'
    `, { yesterday: yesterdayString });
    
    const yesterdayTotalResult = await db.query(`
      SELECT count() AS count
      FROM element_changes
      WHERE timestamp >= <datetime>$yesterday AND timestamp < (<datetime>$yesterday + 1d)
    `, { yesterday: yesterdayString });
    
    // 获取总数据量
    const totalResult = await db.query(`SELECT count() AS total FROM pe`);
    
    // 安全提取计数，处理类型问题
    const getCount = (queryResult: any): number => {
      if (Array.isArray(queryResult) && 
          queryResult.length > 0 && 
          queryResult[0] && 
          typeof queryResult[0] === 'object' && 
          'count' in queryResult[0] &&
          typeof queryResult[0].count === 'number') {
        return queryResult[0].count;
      }
      return 0;
    };
    
    // 构建返回对象
    const today = {
      total: getCount(todayTotalResult),
      add_count: getCount(todayAddResult),
      modify_count: getCount(todayModifyResult),
      delete_count: getCount(todayDeleteResult)
    };
    
    const yesterday = {
      total: getCount(yesterdayTotalResult),
      add_count: getCount(yesterdayAddResult),
      modify_count: getCount(yesterdayModifyResult),
      delete_count: getCount(yesterdayDeleteResult)
    };
    
    // 安全提取总数
    let total = 0;
    if (Array.isArray(totalResult) && 
        totalResult.length > 0 && 
        totalResult[0] && 
        typeof totalResult[0] === 'object' && 
        'total' in totalResult[0] &&
        typeof totalResult[0].total === 'number') {
      total = totalResult[0].total;
    }
    
    return {
      today,
      yesterday,
      total,
    };
  } catch (err) {
    console.error('获取变更总览数据失败:', err);
    return null;
  }
}

/**
 * 获取所有会话信息
 */
export async function getSessions() {
  const db = await getDB();
  if (!db) return [];
  
  try {
    const result = await db.query(`
      SELECT 
        id,
        sesno,
        project,
        timestamp,
        dbnum,
        add_count,
        modify_count,
        delete_count
      FROM sessions
      ORDER BY sesno DESC
    `);
    
    // 确保返回类型是Session[]
    return Array.isArray(result) ? (result as unknown as any[]) : [];
  } catch (err) {
    console.error('获取会话信息失败:', err);
    return [];
  }
}

/**
 * 根据会话ID获取该会话中的所有变更
 * @param sessionId 会话ID
 */
export async function getChangesBySession(sessionId: string) {
  const db = await getDB();
  if (!db) return [];
  
  try {
    const result = await db.query(`
      SELECT 
        id,
        refno,
        operation_type,
        entity_type,
        timestamp,
        sesno,
        details
      FROM element_changes
      WHERE session_id = type::record('sessions', $sessionId)
      ORDER BY timestamp DESC
    `, {
      sessionId,
    });
    
    // 确保返回类型是正确的变更记录数组
    return Array.isArray(result) ? (result as unknown as any[]) : [];
  } catch (err) {
    console.error('获取会话变更记录失败:', err);
    return [];
  }
}