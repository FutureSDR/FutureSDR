#!/usr/bin/env python3
# Joint copyright of Josh Conway and discord user:winter_soldier#1984
# https://gitlab.com/crankylinuxuser/meshtastic_sdr
# License is GPL3 (Gnu public license version 3)
# Modified by bastibl for FutureSDR

import sys
import argparse
import base64
import socket
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.backends import default_backend
from meshtastic import protocols, mesh_pb2

UDP_IP = "127.0.0.1"
UDP_PORT = 55555


def hexStringToBinary(hexString):
    binString = bytes.fromhex(hexString)
    return binString


def parseAESKey(key):
    # We look if there's a "NOKEY" declaration, a key provided, or an absence of key. We do the right thing depending on each choice.
    # The "NOKEY" is basically ham mode. You're forbidden from using encryption.
    # If you dont provide a key, we use the default one. We try to make it easy on our users!
    # Note this format is in Base64

    meshtasticFullKeyBase64 = "1PG7OiApB1nwvP+rz05pAQ=="
    try:
        if key == "0" or key == "NOKEY" or key == "nokey" or key == "NONE" or key == "none" or key == "HAM" or key == "ham":
            meshtasticFullKeyBase64 = "AAAAAAAAAAAAAAAAAAAAAA=="
        elif ( len(args.key) > 0 ):
            meshtasticFullKeyBase64 = args.key
    except:
        pass

    # Validate the key is 128bit/32byte or 256bit/64byte long. Fail if not.
    aesKeyLength = len(base64.b64decode(meshtasticFullKeyBase64).hex())
    if (aesKeyLength == 32 or aesKeyLength == 64):
        pass
    else:
        print("The included AES key appears to be invalid. The key length is" , aesKeyLength , "and is not the key length of 128 or 256 bits.")
        sys.exit()

    # Convert the key FROM Base64 TO hexadecimal.
    return base64.b64decode(meshtasticFullKeyBase64.encode('ascii'))


def dataExtractor(data):
    # Now we split the data into the appropriate meshtastic packet structure using https://meshtastic.org/docs/overview/mesh-algo/
    # NOTE: The data coming out of GnuRadio is MSB or big endian. We have to reverse byte order after this step.

    # destination : 4 bytes 
    # sender      : 4 bytes
    # packetID    : 4 bytes
    # flags       : 1 byte
    # channelHash : 1 byte
    # reserved    : 2 bytes
    # data        : 0-237 bytes
    meshPacketHex = {
        'dest' : hexStringToBinary(data[0:8]),
        'sender' : hexStringToBinary(data[8:16]),
        'packetID' : hexStringToBinary(data[16:24]),
        'flags' : hexStringToBinary(data[24:26]),
        'channelHash' : hexStringToBinary(data[26:28]),
        'reserved' : hexStringToBinary(data[28:32]),
        'data' : hexStringToBinary(data[32:len(data)-4])
    }
    return meshPacketHex


def dataDecryptor(meshPacketHex, meshtasticFullKeyHex):

    # Build the nonce. This is (packetID)+(00000000)+(sender)+(00000000) for a total of 128bit
    # Even though sender is a 32 bit number, internally its used as a 64 bit number.
    # Needs to be a bytes array for AES function.
    aesNonce = meshPacketHex['packetID'] + b'\x00\x00\x00\x00' + meshPacketHex['sender'] + b'\x00\x00\x00\x00'

    # Initialize the cipher
    cipher = Cipher(algorithms.AES(meshtasticFullKeyHex), modes.CTR(aesNonce), backend=default_backend())
    decryptor = cipher.decryptor()

    # Do the decryption. Note, that this cipher is reversible, so running the cipher on encrypted gives decrypted, and running the cipher on decrypted gives encrypted.
    decryptedOutput = decryptor.update(meshPacketHex['data']) + decryptor.finalize()
    return decryptedOutput


def decodeProtobuf(packetData):
    data = mesh_pb2.Data()
    try:
        data.ParseFromString(packetData)
        handler = protocols.get(data.portnum)
        if handler.protobufFactory is None:
            pass
        else:
            pb = handler.protobufFactory()
            pb.ParseFromString(data.payload)
    except:
        data = "INVALID PROTOBUF:"
    return data


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Meshtastic Decoder')
    parser.add_argument('-k', '--key', action='store',dest='key', help='AES key override in Base64')
    args = parser.parse_args()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind((UDP_IP, UDP_PORT))
    aesKey = parseAESKey(args.key)

    while True:
        data, addr = sock.recvfrom(1024)
        extractedData = dataExtractor(data.hex())
        decryptedData = dataDecryptor(extractedData, aesKey)

        protobufMessage = decodeProtobuf(decryptedData)
        if(protobufMessage == "INVALID PROTOBUF:"):
            print("INVALID PROTOBUF: ", end = '')
            print(decryptedData)
        else:
            print(protobufMessage)
