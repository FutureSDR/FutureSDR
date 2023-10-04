import struct

def decodeM17 ( encoded ) :
    charMap = ' ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 -/.'

    # check for unique values
    if encoded == 0xFFFFFFFFFFFF:
        # BROADCAST
        call = 'ALL'
    elif encoded == 0:
        call = 'RESERVED'
    elif encoded >= 0xEE6B28000000:
        call = 'RESERVED'
    else:
        call = ''
    while ( encoded > 0) :
        call = call + charMap [ encoded % 40]
        encoded = encoded // 40

    return call


n = struct.unpack('>Q', bytes([0, 0, 0, 0, 73, 142, 195, 244]))[0]
print(f"n {n}")
r = decodeM17(n)
print(f"decoded {r}")
