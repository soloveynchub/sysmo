#!/usr/bin/env python3
"""Physical SSD/APFS capacity metadata. All diskutil operations are read-only."""
import json
import plistlib
import subprocess
import time


def diskutil(*args):
    result = subprocess.run(['/usr/sbin/diskutil', *args], capture_output=True, timeout=10, check=True)
    if len(result.stdout) > 4 * 1024 * 1024:
        raise ValueError('Disk metadata too large')
    return plistlib.loads(result.stdout)


def layout(root, apfs, physical):
    reference = root.get('APFSContainerReference')
    container = next((c for c in apfs.get('Containers', []) if c.get('ContainerReference') == reference), None)
    if container is None:
        return {'status': 'unavailable', 'ts': time.time()}
    stores = {p['DeviceIdentifier'] for p in container.get('PhysicalStores', [])}
    disks = [d for d in physical.get('AllDisksAndPartitions', [])
             if stores & {p.get('DeviceIdentifier') for p in d.get('Partitions', [])}]
    # Multiple stores (Fusion/RAID) require different accounting; do not guess.
    if len(disks) != 1 or len(stores) != 1:
        return {'status': 'unsupported_layout', 'ts': time.time()}
    disk = disks[0]
    total = int(disk['Size'])
    capacity = int(container['CapacityCeiling'])
    free = int(container['CapacityFree'])
    if not 0 <= free <= capacity <= total:
        raise ValueError('Inconsistent disk capacity')
    roles = {'data': 0, 'system': 0, 'vm': 0, 'service': 0}
    for volume in container.get('Volumes', []):
        tags = volume.get('Roles', [])
        key = 'data' if 'Data' in tags else 'system' if 'System' in tags else 'vm' if 'VM' in tags else 'service'
        roles[key] += max(0, int(volume.get('CapacityInUse', 0)))
    used = capacity - free
    measured = sum(roles.values())
    if measured > used:
        # Shared blocks / changing metadata: show actual total without false segment sizes.
        return dict(status='partial', ts=time.time(), physical_total=total, container_total=capacity,
                    free=free, used=used, segments=[dict(id='used', name='Занято в APFS', size=used),
                    dict(id='partitions', name='Другие разделы / резерв', size=total-capacity),
                    dict(id='free', name='Свободно в основном APFS', size=free)])
    names = {'data': 'Файлы и данные', 'system': 'macOS', 'vm': 'Swap / VM', 'service': 'Preboot, Recovery, Update'}
    segments = [dict(id=k, name=names[k], size=v) for k, v in roles.items() if v]
    segments += [dict(id='metadata', name='Метаданные APFS', size=used-measured),
                 dict(id='partitions', name='Другие разделы / резерв', size=total-capacity),
                 dict(id='free', name='Свободно в основном APFS', size=free)]
    return dict(status='available', ts=time.time(), device=disk['DeviceIdentifier'], physical_total=total,
                container_total=capacity, free=free, used=used, data_volume=roles['data'], segments=segments)


if __name__ == '__main__':
    try:
        print(json.dumps(layout(diskutil('info', '-plist', '/'), diskutil('apfs', 'list', '-plist'),
                                diskutil('list', '-plist', 'internal', 'physical')), ensure_ascii=False))
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        print(json.dumps({'status': 'unavailable', 'ts': time.time()}))
