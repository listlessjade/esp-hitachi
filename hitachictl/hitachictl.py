import asyncio
import json
import logging
import os
import subprocess
import asyncclick as click
from asyncclick_repl import AsyncREPL
import requests
from tqdm import tqdm
from rpc.rpc import Client as RPCClient, wpa2_enterprise_conf, wpa2_personal_conf
from tqdm.utils import CallbackIOWrapper

logging.basicConfig(level=logging.INFO)

client = None
# print()


@click.group(
    cls=AsyncREPL,
    params=[
        click.Option(
            ["--zeroconf/--no-zeroconf"],
            is_flag=True,
            show_default=True,
            default=True,
            help="use Zeroconf/MDNS to discover the hitachi",
        ),
        click.Option(
            ["--use-ble/--no-ble"],
            is_flag=True,
            show_default=True,
            default=True,
            help="use BLEL to connect to the hitachi",
        ),
        click.Option(["--address"], help="Specify the address of the hitachi manually"),
        click.Option(
            ["-i", "--interactive"],
            is_flag=True,
            flag_value=True,
            type=click.types.BoolParamType(),
            help="Run interactive shell",
        ),
    ],
)
@click.pass_context
async def cli(ctx, **kwargs):
    global client

    if ctx.parent:
        params = ctx.parent.params
    else:
        params = ctx.params

    if client is None:
        client = RPCClient()
    await client.find(
        use_ble=params.get("use_ble", True),
        use_mdns=params.get("zeroconf", True),
        hitachi_addr=params.get("address", None),
    )
    pass
    # client = RPCClient()
    # client.find()
    # ctx.obj = ctx.with_resource(client)


@cli.command()
@click.argument("ssid")
@click.argument("password")
async def set_wifi(ssid, password):
    ret = await client.sys_set_wifi(wpa2_personal_conf(ssid, password))
    print(f"Updated wifi config: {ret}")

@cli.command()
async def get_addr():
    print(await client.conn_addresses())

@cli.command()
@click.argument("ssid")
@click.argument("username")
@click.argument("password")
async def set_wifi_enterprise(ssid, username, password):
    ret = await client.sys_set_wifi(wpa2_enterprise_conf(ssid, username, username, password))
    print(f"Updated wifi config: {ret}")

@cli.command()
async def restart():
    ret = await client.sys_restart()
    print(f"Restarted!")


@cli.command()
@click.argument("namespace")
@click.argument("method")
@click.argument("args", nargs=-1)
async def rpc(namespace, method, args):
    print(
        json.dumps(await client.make_call(namespace, method, *args).asdict(), indent=4)
    )

@cli.command()
@click.argument("low", type=click.INT)
@click.argument("high", type=click.INT)
async def wand_set_lovense_mapping(low: int, high: int):
    print(await client.wand_set_lovense_mappings(low, high))

@cli.command()
@click.argument("low", type=click.INT)
@click.argument("high", type=click.INT)
async def wand_set_button_increments(low: int, high: int):
    print(await client.wand_set_button_increments(low, high))

@cli.command()
@click.argument("bottom", type=click.INT)
@click.argument("mid_low", type=click.INT)
@click.argument("mid_high", type=click.INT)
@click.argument("top", type=click.INT)
async def wand_set_light_mappings(bottom: int, mid_low: int, mid_high: int, top: int):
    print(await client.set_light_mappings(bottom, mid_low, mid_high, top))


@cli.command()
@click.argument("percent", type=click.INT)
async def wand_set_percent(percent: int):
    print(await client.wand_set_percent(percent))


@cli.command()
async def wand_get_percent():
    print(await client.wand_get_percent())


@cli.command()
async def build_info():
    print(await client.sys_build_info())


@cli.command()
async def sys_health():
    print(await client.sys_health())


@cli.command()
@click.argument("msg")
async def uart_send(msg: str):
    print(await client.uart_send(msg))


@cli.command()
async def uart_get_last():
    print(await client.uart_get_last())

@cli.command()
async def build_update_firmware():
    subprocess.run(["cargo", "build", "--release"], check=True)
    subprocess.run(
        [
            "espflash",
            "save-image",
            "--chip",
            "esp32c6",
            "-s",
            "8mb",
            "target/riscv32imac-esp-espidf/release/esp-hitachi",
            "/tmp/hitachi.bin",
        ]
    )

    print(await client.http.ota_upload('/tmp/hitachi.bin'))

@cli.command()
@click.argument("new_image", type=click.Path(exists=True,file_okay=True,dir_okay=False))
async def update_firmware(new_image: str):
    print(await client.http.ota_upload(new_image))


if __name__ == "__main__":
    cli()
