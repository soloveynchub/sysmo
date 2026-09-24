#!/usr/bin/env python3
"""Manage the current user's Sysmo LaunchAgent. Never delete monitoring history."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
LABEL = 'local.system-monitor.agent'
TARGET = f'gui/{os.getuid()}/{LABEL}'
PLIST = Path.home() / 'Library/LaunchAgents' / f'{LABEL}.plist'
DATA = Path.home() / 'Library/Application Support/System Monitor'
URL = 'http://127.0.0.1:9899'


def launch(*args, check=True):
    return subprocess.run(['launchctl', *args], check=check, capture_output=True, text=True)


def loaded():
    return launch('print', TARGET, check=False).returncode == 0


def wait_ready():
    for _ in range(120):
        try:
            with urllib.request.urlopen(URL + '/api/system', timeout=1) as response:
                sample = json.load(response)
                if 'cpu' in sample:
                    print('Готово:', URL)
                    return
        except (OSError, ValueError, urllib.error.URLError):
            pass
        time.sleep(1)
    raise RuntimeError(f'Агент не ответил. Проверьте {DATA / "agent-error.log"} и занятость порта 9899.')


def install():
    binary = ROOT / 'agent/target/release/system-monitor-agent'
    if not binary.is_file() or not (ROOT / 'frontend/dist/index.html').is_file():
        raise RuntimeError('Сначала выполните ./scripts/setup.sh')
    DATA.mkdir(parents=True, exist_ok=True)
    DATA.chmod(0o700)
    PLIST.parent.mkdir(parents=True, exist_ok=True)
    configuration = {
        'Label': LABEL, 'ProgramArguments': [str(binary)],
        'EnvironmentVariables': {'SYSTEM_MONITOR_ROOT': str(ROOT), 'SYSTEM_MONITOR_DATA': str(DATA)},
        'RunAtLoad': True, 'KeepAlive': True, 'ThrottleInterval': 10,
        'ProcessType': 'Background', 'WorkingDirectory': str(ROOT),
        'StandardOutPath': str(DATA / 'agent.log'), 'StandardErrorPath': str(DATA / 'agent-error.log'),
    }
    if loaded():
        launch('bootout', TARGET)
    PLIST.write_bytes(plistlib.dumps(configuration))
    PLIST.chmod(0o600)
    launch('bootstrap', f'gui/{os.getuid()}', str(PLIST))
    wait_ready()
    print('Автозапуск при входе включён:', PLIST)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['install', 'start', 'stop', 'restart', 'status', 'uninstall', 'open'])
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('Поддерживается только macOS.')
    if args.action == 'install':
        install()
    elif args.action in ('start', 'restart'):
        if not PLIST.exists():
            raise RuntimeError('Автозапуск не установлен: python3 scripts/manage.py install')
        if not loaded():
            launch('bootstrap', f'gui/{os.getuid()}', str(PLIST))
        elif args.action == 'restart':
            launch('kickstart', '-k', TARGET)
        wait_ready()
    elif args.action in ('stop', 'uninstall'):
        if loaded():
            launch('bootout', TARGET)
        if args.action == 'uninstall':
            PLIST.unlink(missing_ok=True)
        print('Агент остановлен.' + (' Автозапуск удалён.' if args.action == 'uninstall' else ''))
        print('История сохранена:', DATA)
    elif args.action == 'status':
        result = launch('print', TARGET, check=False)
        print('LaunchAgent загружен.' if result.returncode == 0 else 'LaunchAgent остановлен.')
        print('Автозапуск:', 'установлен' if PLIST.exists() else 'не установлен')
        print('Логи и история:', DATA)
        if result.returncode == 0:
            wait_ready()
    elif args.action == 'open':
        wait_ready()
        subprocess.run(['open', URL], check=True)


if __name__ == '__main__':
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'Ошибка: {error}', file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr.strip(), file=sys.stderr)
        sys.exit(1)
