import asyncio
import json
from bleak import BleakClient, BleakScanner
import time
import aioconsole

addr = ""
req_char = "813f9733-95c9-49ba-84a0-d0167c260eef"
res_char = "23ad909d-511b-4fad-ad85-0bf102eee315"

req_id = 0

def response_notify(sender, data):
    print(f"from {sender}: {json.loads(data.decode('utf8'))}")

async def rpc_call(client, req_id, method, params=[]):
    call = {
        "method": method,
        "id": req_id,
        "params": params
    }

    print(f"sending {call}")

    await client.write_gatt_char(req_char, json.dumps(call).encode('utf8'), response=False)


script = open("test.rhai").read()

async def main():
    device = await BleakScanner.find_device_by_filter(lambda device,data: "54300001-0023-4bd4-bbd5-a6920e4c5653" in data.service_uuids)
    async with BleakClient(device.address) as client:
        await client.start_notify(res_char, response_notify)
        await rpc_call(client, 0, "mgmt:set_wifi", [])
        # await rpc_call(client, 0, "set_script", script)
        await asyncio.sleep(5)

        # while True:
        #     percent = int(await aioconsole.ainput('Percent > '))
        #     await rpc_call(client, 1, "set_percent", [percent])
            
        # await rpc_call(client, 1, "meow", [0])
        # await rpc_call(client, 2, "meow", [1])
        # await rpc_call(client, 3, "meow", ["nyam"])
        # while True:
            # await asyncio.sleep(1)
        

asyncio.run(main())