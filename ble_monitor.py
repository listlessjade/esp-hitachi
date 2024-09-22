import asyncio
from bleak import BleakClient, BleakScanner

req_char = "813f9733-95c9-49ba-84a0-d0167c260eef"
res_char = "23ad909d-511b-4fad-ad85-0bf102eee315"
log_char = "b170b38a-eff7-4883-b946-50e07c390200"
req_id = 0

def log_notify(sender, data):
    print(data.decode('utf8'))

async def main():
    device = await BleakScanner.find_device_by_filter(lambda device,data: "54300001-0023-4bd4-bbd5-a6920e4c5653" in data.service_uuids)
    async with BleakClient(device.address) as client:
        await client.start_notify(log_char, log_notify)
        while True:
            await asyncio.sleep(1)

asyncio.run(main())