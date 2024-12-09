from abc import ABC, abstractmethod
from asyncio import Future
import asyncio
from dataclasses import dataclass
import functools
import json
import os
import typing
from typing import Any, Optional
import logging
import aiofiles
import aiohttp
from bleak import BleakClient, BleakScanner
import requests
from tqdm import tqdm
from tqdm.utils import CallbackIOWrapper
from urllib.parse import urljoin
from zeroconf._utils import ipaddress
from zeroconf.asyncio import AsyncZeroconf as Zeroconf
import atexit

@dataclass
class RPCResponse[T]:
     res_id: int
     result: Optional[T] = None
     error: Optional[str] = None

class RPCClient(ABC):
    @abstractmethod
    async def discover(self) -> bool:
        pass

    @abstractmethod
    async def rpc_call[T](self, namespace: str, method: str, *args) -> RPCResponse[T]:
        pass

    @abstractmethod
    async def is_alive(self) -> bool:
        pass

async def upload_with_progress(path: str):
    chunk_size = 1024 * 8
    size = os.path.getsize(path)
    total_read = 0
    with tqdm(total=size, unit="B", unit_scale=True, unit_divisor=1024) as bar:
        async with aiofiles.open(path, "rb") as f:
            while chunk := await f.read(chunk_size):
                bar.update(len(chunk))
                yield chunk


class HTTPRpc(RPCClient):
    MDNS_TYPE = "_magicwandrpc._tcp.local."
    MDNS_NAME = "Magic Wand [v0.1]"

    def __init__(self):
        self.address = None
        self.session = aiohttp.ClientSession()
        atexit.register(self.cleanup)

    def cleanup(self):
        try:
            loop = asyncio.get_event_loop()
            asyncio.create_task(self._cleanup())
        except RuntimeError:
            loop = asyncio.new_event_loop()
            loop.run_until_complete(self._cleanup())

    async def _cleanup(self):
        await self.session.close()

    def route(self, route: str) -> str:
        return urljoin(
            self.address,
            route
        )
    
    async def discover(self, use_mdns=True, hitachi_addr=None) -> bool:
        if hitachi_addr:
            self.address = hitachi_addr
            return await self.is_alive()
        
        if use_mdns:      
            zeroconf = Zeroconf()
            try:
                service = await zeroconf.async_get_service_info(HTTPRpc.MDNS_TYPE, HTTPRpc.MDNS_NAME + "." + HTTPRpc.MDNS_TYPE)
                ip = str(
                    ipaddress.get_ip_address_object_from_record(service.dns_addresses()[0])
                )
                port = service.port
                logging.info(f"[MDNS] hitachi is at {ip}:{port}")
                self.address = f"http://{ip}:{port}"
                return True
            except Exception as e:
                logging.error(f"[MDNS] didn't find hitachi: {e}")
                await zeroconf.async_close()
                return False


    async def rpc_call[T](self, namespace: str, method: str,  *args) -> RPCResponse[T]:
        async with self.session.post(
            self.route("/rpc"),
            data=json.dumps({
                'method': f"{namespace}:{method}",
                'id': 0,
                'params': list(*args)
            }),
        ) as res:
            return RPCResponse(**(await res.json()))

    async def is_alive(self) -> bool:
        if self.address is None:
            return False
        try:
            async with self.session.get(self.route("/check")) as res:    
                res.raise_for_status()
                return True
        except aiohttp.ClientError:
            return False
        
    async def ota_upload(self, file: str) -> str:
        async with self.session.post(self.route("/ota/upload"), data=upload_with_progress(file), headers={"Content-Type": "application/octet-stream", "Content-Length": str(os.path.getsize(file))}) as res:
            res.raise_for_status()
            return await res.text()
    
class BLERpc(RPCClient):
    REQ_CHAR = "813f9733-95c9-49ba-84a0-d0167c260eef"
    RES_CHAR = "23ad909d-511b-4fad-ad85-0bf102eee315"

    def __init__(self):
        self.pending_requests: dict[int, Future[RPCResponse[Any]]] = {}
        self.conn: Optional[BleakClient] = None
        self.id_counter = 0

    async def discover(self, rediscover=False):
        if not rediscover and self.conn is not None:
            return
        
        device = await BleakScanner.find_device_by_filter(
            lambda device, data: "54300001-0023-4bd4-bbd5-a6920e4c5653"
            in data.service_uuids
        )

        logging.info(f"Return from BLE scanner: {device}")
        if not device:
            return False
        
        self.conn = BleakClient(device.address)
        await self.conn.connect()
        await self.conn.start_notify(BLERpc.RES_CHAR, self.response_notify)
        logging.info("Connected to BLE!")
        return True

    def response_notify(self, sender: Any, data: bytearray):
        res = RPCResponse(**json.loads(data.decode("utf8")))
        req = self.pending_requests.get(res.res_id)
        if not req:
            logging.warning(f"Orphaned response: {res}")
            return

        req.set_result(res)

    def make_id(self) -> int:
        while True:
            if self.id_counter >= 255:
                self.id_counter = 0

            if self.id_counter not in self.pending_requests:
                return self.id_counter

            self.id_counter += 1

    async def rpc_call[T](self, namespace: str, method: str, *args) -> RPCResponse[T]:
        assert await self.is_alive()
        req_id = self.make_id()
        self.pending_requests[req_id] = asyncio.get_running_loop().create_future()
        print(json.dumps(
                {"method": f"{namespace}:{method}", "id": req_id, "params": list(*args)}
            ))

        await self.conn.write_gatt_char(
            BLERpc.REQ_CHAR,
            json.dumps(
                {"method": f"{namespace}:{method}", "id": req_id, "params": list(*args)}
            ).encode("utf8"),
            response=False,
        )

        try:
            res = await self.pending_requests[req_id]
            return res
        finally:
            del self.pending_requests[req_id]

    async def is_alive(self) -> bool:
        return self.conn is not None
    