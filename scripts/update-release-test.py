#!/usr/bin/env python3
import base64
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('feed',Path(__file__).with_name('prepare-update-release.py'))
feed=importlib.util.module_from_spec(spec);spec.loader.exec_module(feed)

class FeedTest(unittest.TestCase):
    def fixture(self,root):
        version='1.2.0';tag='v'+version
        for t in ['aarch64-apple-darwin','x86_64-apple-darwin','x86_64-pc-windows-msvc','x86_64-unknown-linux-gnu']:
            (root/f'aillies-{tag}-{t}.tar.gz').write_bytes(b'fixture')
        for n in [f'Aillies-{tag}-source.zip',f'Aillies_{version}_aarch64.dmg',f'Aillies_{version}_amd64.deb']:
            (root/n).write_bytes(b'fixture')
        for n in [f'Aillies_{version}_aarch64.app.tar.gz',f'Aillies_{version}_x64-setup.exe',f'Aillies_{version}_amd64.AppImage']:
            (root/n).write_bytes(b'fixture')
            (root/(n+'.sig')).write_text(base64.b64encode(b'untrusted comment: fixture\nsignature\ntrusted comment: timestamp:1\tfile:fixture\tversion:1.2.0\nglobal\n').decode())
    def test_complete_feed_and_checksums(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);self.fixture(root)
            result=feed.prepare(root,'1.2.0','v1.2.0')
            self.assertEqual(set(result['platforms']),{'darwin-aarch64','windows-x86_64','linux-x86_64'})
            self.assertEqual(len((root/'SHA256SUMS').read_text().splitlines()),14)
    def test_missing_package_and_bad_signature_never_publish_feed(self):
        for issue in ['missing','wrong_version','duplicate']:
            with self.subTest(issue=issue),tempfile.TemporaryDirectory() as d:
                root=Path(d);self.fixture(root)
                package=root/'Aillies_1.2.0_x64-setup.exe'
                if issue=='missing':package.unlink()
                elif issue=='wrong_version':Path(str(package)+'.sig').write_text(base64.b64encode(b'trusted comment: version:0.0.1').decode())
                else:
                    (root/'duplicate').mkdir();(root/'duplicate'/package.name).write_bytes(b'fixture')
                with self.assertRaises(ValueError):feed.prepare(root,'1.2.0','v1.2.0')
                self.assertFalse((root/'latest.json').exists())

if __name__=='__main__':unittest.main()
