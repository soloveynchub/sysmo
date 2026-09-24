import unittest
from disk_layout import layout


class DiskTests(unittest.TestCase):
    def value(self, volume=600):
        return layout({'APFSContainerReference': 'disk3'}, {'Containers': [{'ContainerReference': 'disk3',
            'PhysicalStores': [{'DeviceIdentifier': 'disk0s2'}], 'CapacityCeiling': 900, 'CapacityFree': 200,
            'Volumes': [{'Roles': ['Data'], 'CapacityInUse': volume}, {'Roles': ['System'], 'CapacityInUse': 90}]}]},
            {'AllDisksAndPartitions': [{'DeviceIdentifier': 'disk0', 'Size': 1000, 'Partitions': [{'DeviceIdentifier': 'disk0s2'}]}]})

    def test_segments_reconcile_full_physical_disk(self):
        v = self.value()
        self.assertEqual(v['physical_total'], 1000)
        self.assertEqual(v['container_total'], 900)
        self.assertEqual(sum(s['size'] for s in v['segments']), 1000)
        self.assertEqual(next(s['size'] for s in v['segments'] if s['id'] == 'metadata'), 10)
        self.assertEqual(next(s['size'] for s in v['segments'] if s['id'] == 'partitions'), 100)

    def test_inconsistent_volume_sum_does_not_invent_segmentation(self):
        v = self.value(800)
        self.assertEqual(v['status'], 'partial')
        self.assertEqual(sum(s['size'] for s in v['segments']), 1000)
        self.assertFalse(any(s['size'] < 0 for s in v['segments']))


if __name__ == '__main__':
    unittest.main()
