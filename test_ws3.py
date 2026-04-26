import asyncio
import websockets

urls = [
    "wss://stream-toko.2meta.app/ws/btcbidr@kline_1m",
    "wss://www.tokocrypto.com/ws/btcbidr@kline_1m"
]

async def test():
    for url in urls:
        print(f"Testing {url} ...")
        try:
            async with websockets.connect(url, ping_timeout=None) as ws:
                print(f"SUCCESS connected to {url}")
                msg = await asyncio.wait_for(ws.recv(), timeout=5.0)
                print("Received:", msg)
                return
        except Exception as e:
            print(f"Failed {url}: {e}")

asyncio.run(test())
