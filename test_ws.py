import asyncio
import websockets
import json

async def test():
    try:
        async with websockets.connect('wss://stream.binance.com:9443/ws/btcbidr@kline_1m') as ws:
            print("Connected to Binance")
            msg = await asyncio.wait_for(ws.recv(), timeout=5.0)
            print("Received:", msg)
    except Exception as e:
        print("Binance failed:", e)

asyncio.run(test())
