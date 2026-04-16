import struct

with open('D:/work/plant-code/pdms-io-fork/test-file/attlib.dat', 'rb') as f:
    data = f.read()

page1 = struct.unpack('>512I', data[2048:4096])
atnain_start = page1[3]
print(f'ATNAIN starts at page {atnain_start}')

pipe_hash = 0x000463E9
elbo_hash = 0x0001773B
tee_hash = 0x00003557
known_hashes = {pipe_hash: 'PIPE', elbo_hash: 'ELBO', tee_hash: 'TEE',
                0x0005EA62: 'VALV', 0x00018657: 'EQUI', 0x0000B900: 'BRAN',
                0x00054F30: 'STRT', 0x0003EB8A: 'NOZZ'}

all_noun_hashes = set()
found = {}

for pg in range(atnain_start, atnain_start + 100):
    offset = pg * 2048
    if offset + 2048 > len(data):
        break
    vals = struct.unpack('>512I', data[offset:offset + 2048])
    for i in range(0, len(vals) - 2, 3):
        nh, ai, tc = vals[i], vals[i + 1], vals[i + 2]
        if nh == 0 or nh == 0xFFFFFFFF:
            continue
        all_noun_hashes.add(nh)
        if nh in known_hashes:
            name = known_hashes[nh]
            if name not in found:
                found[name] = []
            found[name].append((ai, tc))

print(f'Total unique noun hashes: {len(all_noun_hashes)}')
for name, entries in found.items():
    print(f'{name}: {len(entries)} entries - {entries[:5]}')

if not found:
    hashes_sorted = sorted(all_noun_hashes)
    print(f'Hash range: 0x{hashes_sorted[0]:08X} .. 0x{hashes_sorted[-1]:08X}')
    print(f'First 20 hashes:')
    for h in hashes_sorted[:20]:
        print(f'  0x{h:08X}')
    
    # Check if PIPE hash exists anywhere in the file
    pipe_bytes = struct.pack('>I', pipe_hash)
    pos = data.find(pipe_bytes)
    if pos >= 0:
        page_num = pos // 2048
        offset_in_page = pos % 2048
        print(f'\nPIPE hash found at byte offset {pos} (page {page_num}, offset {offset_in_page})')
        ctx = struct.unpack('>8I', data[pos-4:pos+28])
        print(f'Context: {[hex(v) for v in ctx]}')
    else:
        print('\nPIPE hash NOT found in entire file!')
