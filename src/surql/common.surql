remove function fn::latest_pe_sesno;
define function fn::latest_pe_sesno($pe: record, $s: option<number>, $db: option<number>){
    let $sesno = $pe.sesno?:0;
    let $r = return if $s != none and $db != none {
        #找到对应的时间
        let $t = ses:[$db, $s].date;
        return select value sesno from only pe_ses_h:[record::id($pe), 0]..[record::id($pe)] where ses.date<=$t ORDER BY sesno DESC limit 1;
    } else if $s != none {
        return if $s >= $sesno {
            return $sesno;
        } else{
            return select value sesno from only pe_ses_h:[record::id($pe), 0]..[record::id($pe)] where sesno<=$s ORDER BY sesno DESC limit 1;
        };
    } else {
        return $sesno
    };

    #需要知道是否是 latest，latest 都在 pe 表里存储了
    let $l = return if $r >= $sesno { 1 } else { 0 } ;

    return [$r?:0, $l];
};

remove function fn::latest_pe;
define function fn::latest_pe($pe: record, $s: option<number>, $db: option<number>){
    let $s = fn::latest_pe_sesno($pe, $s, $db);
    return if $s[1]  {
        return $pe;
    } else{
        let $id = record::id($pe);
    return type::record('pe', [$id, $s[0]]);
    }
};


-- fn::latest_pe_sesno(pe:17496_171715, 880, 1112);
-- fn::latest_pe_sesno(pe:17496_171715, 880);
-- fn::latest_pe_sesno(pe:17496_171715);
