#!/usr/bin/env python3
import os

lines = []
lines.append('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 800">')
lines.append('  <defs>')
lines.append('    <marker id="arrow" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto"><polygon points="0 0, 10 3.5, 0 7" fill="#2563eb"/></marker>')
lines.append('    <marker id="ag" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto"><polygon points="0 0, 10 3.5, 0 7" fill="#059669"/></marker>')
lines.append('    <marker id="ao" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto"><polygon points="0 0, 10 3.5, 0 7" fill="#ea580c"/></marker>')
lines.append('    <marker id="ap" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto"><polygon points="0 0, 10 3.5, 0 7" fill="#7c3aed"/></marker>')
lines.append('    <filter id="sh"><feDropShadow dx="1" dy="1" stdDeviation="2" flood-color="#00000020"/></filter>')
lines.append('  </defs>')
lines.append('  <style>text{font-family:"Segoe UI","Microsoft YaHei",sans-serif}.t{font-size:18px;font-weight:bold;fill:#1e293b}.st{font-size:13px;fill:#64748b}.l{font-size:13px;fill:#1e293b;font-weight:600}.sl{font-size:11px;fill:#64748b}.c{font-family:"Cascadia Code",Consolas,monospace;font-size:11px;fill:#334155}</style>')
lines.append('  <rect width="1200" height="800" fill="#fafafa"/>')
lines.append('  <text x="600" y="32" text-anchor="middle" class="t">E3D/PDMS NOUN Attribute Metadata Architecture</text>')
lines.append('  <text x="600" y="50" text-anchor="middle" class="st">From Disk Storage to Runtime Dictionary</text>')

# Storage Layer
lines.append('  <rect x="40" y="70" width="340" height="260" rx="8" fill="none" stroke="#94a3b8" stroke-dasharray="6,3"/>')
lines.append('  <text x="55" y="90" class="l" fill="#475569">Storage: attlib.dat</text>')
lines.append('  <rect x="60" y="105" width="140" height="60" rx="6" fill="#dbeafe" stroke="#3b82f6" filter="url(#sh)"/>')
lines.append('  <text x="130" y="128" text-anchor="middle" class="l">ATNAIN</text>')
lines.append('  <text x="130" y="143" text-anchor="middle" class="sl">[NounHash,AttrIdx,Type]</text>')
lines.append('  <rect x="220" y="105" width="140" height="60" rx="6" fill="#dbeafe" stroke="#3b82f6" filter="url(#sh)"/>')
lines.append('  <text x="290" y="128" text-anchor="middle" class="l">ATGTDF</text>')
lines.append('  <text x="290" y="143" text-anchor="middle" class="sl">[Hash,DataType,Defi]</text>')
lines.append('  <rect x="60" y="180" width="140" height="50" rx="6" fill="#e0e7ff" stroke="#6366f1" filter="url(#sh)"/>')
lines.append('  <text x="130" y="203" text-anchor="middle" class="l">Attr Records</text>')
lines.append('  <text x="130" y="218" text-anchor="middle" class="sl">name, desc, category</text>')
lines.append('  <rect x="220" y="180" width="140" height="50" rx="6" fill="#e0e7ff" stroke="#6366f1" filter="url(#sh)"/>')
lines.append('  <text x="290" y="203" text-anchor="middle" class="l">ATGTSX/IX</text>')
lines.append('  <text x="290" y="218" text-anchor="middle" class="sl">index/aux tables</text>')
lines.append('  <rect x="100" y="248" width="200" height="35" rx="5" fill="#f1f5f9" stroke="#94a3b8"/>')
lines.append('  <text x="200" y="270" text-anchor="middle" class="sl">Page 1 Directory</text>')

# Runtime Layer
lines.append('  <rect x="420" y="70" width="380" height="260" rx="8" fill="none" stroke="#f97316" stroke-dasharray="6,3"/>')
lines.append('  <text x="435" y="90" class="l" fill="#c2410c">Runtime: core.dll</text>')
lines.append('  <rect x="440" y="105" width="170" height="75" rx="8" fill="#fef3c7" stroke="#f59e0b" stroke-width="2" filter="url(#sh)"/>')
lines.append('  <text x="525" y="125" text-anchor="middle" class="l">DB_Noun::dictionary_</text>')
lines.append('  <text x="525" y="143" text-anchor="middle" class="c">map&lt;int, DB_Noun*&gt;</text>')
lines.append('  <text x="525" y="160" text-anchor="middle" class="sl">1931 entries (RB-tree)</text>')
lines.append('  <rect x="630" y="105" width="155" height="75" rx="8" fill="#fef3c7" stroke="#f59e0b" stroke-width="2" filter="url(#sh)"/>')
lines.append('  <text x="708" y="125" text-anchor="middle" class="l">DB_Attribute::</text>')
lines.append('  <text x="708" y="143" text-anchor="middle" class="l">dictionary_</text>')
lines.append('  <text x="708" y="160" text-anchor="middle" class="sl">Lazy-loaded cache</text>')
lines.append('  <rect x="440" y="205" width="170" height="50" rx="6" fill="#dcfce7" stroke="#16a34a" filter="url(#sh)"/>')
lines.append('  <text x="525" y="228" text-anchor="middle" class="l">getSystemAttributes()</text>')
lines.append('  <text x="525" y="243" text-anchor="middle" class="sl">set&lt;int&gt; attr_hashes</text>')
lines.append('  <rect x="630" y="205" width="155" height="50" rx="6" fill="#dcfce7" stroke="#16a34a" filter="url(#sh)"/>')
lines.append('  <text x="708" y="228" text-anchor="middle" class="l">db_get_attr_list</text>')
lines.append('  <text x="708" y="243" text-anchor="middle" class="sl">opcode=60</text>')

# Arrows Storage->Runtime
lines.append('  <line x1="360" y1="135" x2="430" y2="135" stroke="#2563eb" stroke-width="2" marker-end="url(#arrow)"/>')
lines.append('  <line x1="525" y1="180" x2="525" y2="200" stroke="#059669" stroke-width="1.5" marker-end="url(#ag)"/>')
lines.append('  <line x1="610" y1="230" x2="625" y2="230" stroke="#059669" stroke-width="1.5" marker-end="url(#ag)"/>')

# Rust Layer
lines.append('  <rect x="40" y="360" width="760" height="190" rx="8" fill="none" stroke="#7c3aed" stroke-dasharray="6,3"/>')
lines.append('  <text x="55" y="380" class="l" fill="#6d28d9">Rust: pdms-io-fork</text>')
lines.append('  <rect x="60" y="395" width="160" height="65" rx="6" fill="#ede9fe" stroke="#8b5cf6" filter="url(#sh)"/>')
lines.append('  <text x="140" y="418" text-anchor="middle" class="l">AttlibData::</text>')
lines.append('  <text x="140" y="433" text-anchor="middle" class="l">parse_attlib_file()</text>')
lines.append('  <text x="140" y="448" text-anchor="middle" class="sl">offline ATNAIN parser</text>')
lines.append('  <rect x="240" y="395" width="160" height="65" rx="6" fill="#ede9fe" stroke="#8b5cf6" filter="url(#sh)"/>')
lines.append('  <text x="320" y="418" text-anchor="middle" class="l">NounSchema::</text>')
lines.append('  <text x="320" y="433" text-anchor="middle" class="l">from_attlib()</text>')
lines.append('  <text x="320" y="448" text-anchor="middle" class="sl">noun_hash to attrs</text>')
lines.append('  <rect x="420" y="395" width="175" height="65" rx="8" fill="#fce7f3" stroke="#db2777" stroke-width="2" filter="url(#sh)"/>')
lines.append('  <text x="508" y="415" text-anchor="middle" class="l">all_attr_info.json</text>')
lines.append('  <text x="508" y="433" text-anchor="middle" class="sl">339 NOUNs, 6555 attrs</text>')
lines.append('  <text x="508" y="448" text-anchor="middle" class="sl">offset + type + default</text>')
lines.append('  <rect x="615" y="395" width="170" height="65" rx="6" fill="#ede9fe" stroke="#8b5cf6" filter="url(#sh)"/>')
lines.append('  <text x="700" y="418" text-anchor="middle" class="l">ElementRecord</text>')
lines.append('  <text x="700" y="433" text-anchor="middle" class="l">Reader</text>')
lines.append('  <text x="700" y="448" text-anchor="middle" class="sl">binary to attributes</text>')

# Arrows in Rust layer
lines.append('  <line x1="200" y1="270" x2="140" y2="390" stroke="#7c3aed" stroke-width="1.5" marker-end="url(#ap)"/>')
lines.append('  <line x1="220" y1="427" x2="235" y2="427" stroke="#7c3aed" stroke-width="1.5" marker-end="url(#ap)"/>')
lines.append('  <line x1="400" y1="427" x2="415" y2="427" stroke="#ea580c" stroke-width="2" marker-end="url(#ao)"/>')
lines.append('  <line x1="595" y1="427" x2="610" y2="427" stroke="#7c3aed" stroke-width="1.5" marker-end="url(#ap)"/>')

# Element Record Layout
lines.append('  <rect x="40" y="570" width="760" height="200" rx="8" fill="none" stroke="#0ea5e9" stroke-dasharray="6,3"/>')
lines.append('  <text x="55" y="590" class="l" fill="#0284c7">Element Record (PIPE implicit area)</text>')

y = 610
blocks = [
    (80, "#f1f5f9", "#94a3b8", "Header", "[0..10]"),
    (55, "#dbeafe", "#3b82f6", "PURP", "[11]"),
    (55, "#fef3c7", "#f59e0b", "BOOL*3", "[12]"),
    (65, "#dcfce7", "#16a34a", "BORE", "[13..14]"),
    (65, "#dcfce7", "#16a34a", "TEMP", "[15..16]"),
    (65, "#dcfce7", "#16a34a", "PRES", "[17..18]"),
    (65, "#e0e7ff", "#6366f1", "PSPE", "[19..20]"),
    (65, "#e0e7ff", "#6366f1", "ISPE", "[21..22]"),
    (50, "#f1f5f9", "#94a3b8", "...", "[23+]"),
]
x = 60
for w, fc, sc, lab, sub in blocks:
    lines.append(f'  <rect x="{x}" y="{y}" width="{w}" height="40" rx="4" fill="{fc}" stroke="{sc}"/>')
    lines.append(f'  <text x="{x+w//2}" y="{y+18}" text-anchor="middle" class="sl">{lab}</text>')
    lines.append(f'  <text x="{x+w//2}" y="{y+32}" text-anchor="middle" class="c">{sub}</text>')
    x += w + 5

# BOOL detail
y2 = 670
lines.append(f'  <text x="60" y="{y2+5}" class="l" fill="#475569">BOOL Bit Packing (word[12]):</text>')
bx = 260
for bit, name in [(0, "BUIL"), (1, "SHOP"), (2, "LISS")]:
    lines.append(f'  <rect x="{bx}" y="{y2-8}" width="45" height="28" rx="3" fill="#fef3c7" stroke="#f59e0b"/>')
    lines.append(f'  <text x="{bx+22}" y="{y2+8}" text-anchor="middle" class="c">bit{bit}</text>')
    lines.append(f'  <text x="{bx+22}" y="{y2+25}" text-anchor="middle" class="sl">{name}</text>')
    bx += 50

# Members/Explicit
y3 = 720
for bx2, w2, lab2, sc2 in [(60,120,"Members (0x0002)","#db2777"),(190,120,"Explicit (0x0001)","#db2777"),(320,80,"UDA","#94a3b8")]:
    lines.append(f'  <rect x="{bx2}" y="{y3}" width="{w2}" height="30" rx="4" fill="#fce7f3" stroke="{sc2}"/>')
    lines.append(f'  <text x="{bx2+w2//2}" y="{y3+20}" text-anchor="middle" class="sl">{lab2}</text>')

# Legend
lines.append('  <rect x="830" y="70" width="340" height="290" rx="8" fill="#ffffff" stroke="#e2e8f0"/>')
lines.append('  <text x="845" y="92" class="l">Legend</text>')
ly = 110
for fc, sc, lab in [("#dbeafe","#3b82f6","attlib.dat tables"),("#fef3c7","#f59e0b","Runtime dictionaries"),("#dcfce7","#16a34a","DOUBLE attributes"),("#e0e7ff","#6366f1","ELEMENT references"),("#ede9fe","#8b5cf6","Rust parsers"),("#fce7f3","#db2777","JSON / record blocks")]:
    lines.append(f'  <rect x="845" y="{ly}" width="20" height="14" rx="3" fill="{fc}" stroke="{sc}"/>')
    lines.append(f'  <text x="875" y="{ly+12}" class="sl">{lab}</text>')
    ly += 22
ly += 10
lines.append(f'  <text x="845" y="{ly}" class="l">Offset Encoding:</text>')
ly += 16
lines.append(f'  <text x="845" y="{ly}" class="c">off &lt; 0x100000: word_offset</text>')
ly += 16
lines.append(f'  <text x="845" y="{ly}" class="c">off &gt;= 0x100000:</text>')
ly += 14
lines.append(f'  <text x="855" y="{ly}" class="sl">bit=(off&gt;&gt;20) word=(off&amp;0xFFFFF)</text>')
ly += 20
lines.append(f'  <text x="845" y="{ly}" class="l">Data Sizes (words):</text>')
ly += 16
for t,w in [("INTEGER/WORD/BOOL","1"),("DOUBLE","2"),("ELEMENT","2"),("POSITION","6"),("ORIENTATION","9")]:
    lines.append(f'  <text x="855" y="{ly}" class="sl">{t}: {w} word(s)</text>')
    ly += 14

# Hash + Stats
lines.append('  <rect x="830" y="380" width="340" height="95" rx="8" fill="#f8fafc" stroke="#e2e8f0"/>')
lines.append('  <text x="845" y="400" class="l">db1_hash:</text>')
lines.append('  <text x="845" y="418" class="c">h = 0</text>')
lines.append('  <text x="845" y="433" class="c">for ch in NAME.upper():</text>')
lines.append('  <text x="845" y="448" class="c">  h = (h*26+ord(ch)-64) &amp; 0xFFFFFFFF</text>')
lines.append('  <text x="845" y="463" class="sl">Reversible via db1_dehash()</text>')

lines.append('  <rect x="830" y="490" width="340" height="80" rx="8" fill="#f0fdf4" stroke="#86efac"/>')
lines.append('  <text x="845" y="510" class="l">Statistics:</text>')
lines.append('  <text x="845" y="528" class="sl">NOUN types (IDA): 1932</text>')
lines.append('  <text x="845" y="543" class="sl">NOUN in all_attr_info: 339 (6555 attrs)</text>')
lines.append('  <text x="845" y="558" class="sl">dictionary_ @ 0x5ADD359C (RB-tree)</text>')

lines.append('</svg>')

out = os.path.dirname(os.path.abspath(__file__))
svg_path = os.path.join(out, 'noun_attr_metadata_architecture.svg')
with open(svg_path, 'w', encoding='utf-8') as f:
    f.write('\n'.join(lines))
print(f'SVG written to {svg_path}')
