from pathlib import Path
import os,tomllib,zipfile
root=Path(__file__).resolve().parent.parent
local_cargo=root/'.tools/rust/cargo'
cargo_home=Path(os.environ.get('CARGO_HOME') or (local_cargo if local_cargo.exists() else Path.home()/'.cargo'))
registry=cargo_home/'registry'
if not registry.is_dir():
 raise SystemExit('Cargo registry not found. Build the browser first, or set CARGO_HOME.')
packages=tomllib.loads((root/'rust/Cargo.lock').read_text())['package']
out=root/'rust/assets/ThirdParty.zip'
with zipfile.ZipFile(out,'w',zipfile.ZIP_DEFLATED,compresslevel=9,strict_timestamps=False) as z:
 for package in packages:
  name=package['name']+'-'+package['version']
  for folder in (registry/'src').glob('*/'+name):
   for file in folder.iterdir():
    if file.is_file() and (file.name.upper().startswith(('LICENSE','COPYING','NOTICE','AUTHORS'))):z.write(file,'licenses/'+name+'/'+file.name)
  if package['name']=='adblock':
   for source in (registry/'cache').glob('*/'+name+'.crate'):z.write(source,'source/'+source.name)
 for file in (root/'rust/assets/protection').iterdir():
  if file.is_file():z.write(file,'protection/'+file.name)
 z.writestr('README.txt','Slim Browser native blocking uses adblock-rust 0.13.5 (Brave, MPL-2.0), with ABP-format filter lists. Filter sources, their notices and the unmodified library source archive are bundled. See protection/NOTICE.md. Source repository: https://github.com/brave/adblock-rust/tree/v0.13.5 . Other libraries are identified by the matching licence directories and Cargo.lock in the project.')
print(out)
