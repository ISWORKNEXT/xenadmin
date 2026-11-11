#!/usr/bin/env python3
"""Verify published desktop artifacts and bind workflow receipts to a local tag.
Read-only GitHub calls; downloads/extracts into the explicitly supplied directory.
"""
import argparse,hashlib,json,re,subprocess,tarfile
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('tag');parser.add_argument('release_run',type=int);parser.add_argument('checks_run',type=int)
parser.add_argument('--download-dir',type=Path,required=True);parser.add_argument('--evidence-dir',type=Path,required=True)
args=parser.parse_args()
if not re.fullmatch(r'v\d+\.\d+\.\d+',args.tag):raise ValueError('Expected a version tag')
repo='ISWORKNEXT/xenadmin'
def gh_json(*command):return json.loads(subprocess.check_output(['gh',*command,'--repo',repo],text=True))
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
commit=subprocess.check_output(['git','rev-parse',args.tag+'^{commit}'],text=True).strip()
release=gh_json('release','view',args.tag,'--json','tagName,isDraft,isPrerelease,publishedAt,url,assets')
assert release['tagName']==args.tag and not release['isDraft']
args.evidence_dir.mkdir(parents=True,exist_ok=True);args.download_dir.mkdir(parents=True,exist_ok=True)
for run in [args.release_run,args.checks_run]:
 data=gh_json('run','view',str(run),'--json','status,conclusion,jobs,url,headSha')
 assert data['status']=='completed' and data['conclusion']=='success' and data['headSha']==commit
 (args.evidence_dir/(str(run)+'.json')).write_text(json.dumps(data,indent=2)+'\n')
names=['SHA256SUMS',f'xen-desktop-{args.tag}-linux-x86_64.tar.gz',f'xen-desktop-{args.tag}-windows-x86_64.zip']
assets={a['name']:a for a in release['assets']}
for name in names:
 assert name in assets and assets[name]['state']=='uploaded'
 if not (args.download_dir/name).exists():subprocess.run(['gh','release','download',args.tag,'--repo',repo,'--dir',str(args.download_dir),'--pattern',name],check=True)
 assert assets[name]['digest']=='sha256:'+digest(args.download_dir/name)
expected={name:sha for sha,name in (line.split() for line in (args.download_dir/'SHA256SUMS').read_text().splitlines())}
verified=[]
for name in names[1:]:
 actual=digest(args.download_dir/name);assert expected[name]==actual
 verified.append({'name':name,'sha256':actual,'bytes':(args.download_dir/name).stat().st_size,'published_asset_digest_matches':True})
package=f'xen-desktop-{args.tag}-linux-x86_64'
with tarfile.open(args.download_dir/names[1]) as archive:
 for member in archive.getmembers():
  assert (member.name==package or member.name.startswith(package+'/')) and '..' not in Path(member.name).parts and (member.isdir() or member.isreg())
 archive.extractall(args.download_dir,filter='data')
 binary=args.download_dir/package/'xen-desktop'
 stream=archive.extractfile(package+'/xen-desktop');assert stream is not None
 assert hashlib.sha256(stream.read()).hexdigest()==digest(binary)
receipt={'tag':args.tag,'commit':commit,'url':release['url'],'verified_assets':verified,'linux_binary':str(binary.resolve()),'linux_binary_sha256':digest(binary),'ci_and_release_success':True}
(args.evidence_dir/'release.json').write_text(json.dumps(release,indent=2)+'\n')
(args.evidence_dir/'artifact-checksums.json').write_text(json.dumps(verified,indent=2)+'\n')
(args.evidence_dir/'verification.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
