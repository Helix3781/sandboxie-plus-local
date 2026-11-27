import re, xml.etree.ElementTree as ET

p = r"D:\kaifa-soft\_build\Sandboxie\Sandboxie\core\drv\SboxDrv.vcxproj"
ns = {'m': 'http://schemas.microsoft.com/developer/msbuild/2003'}
tree = ET.parse(p)
root = tree.getroot()

def excluded(item):
    for e in item.findall('m:ExcludedFromBuild', ns):
        c = e.get('Condition') or ''
        if 'SbieRelease|x64' in c:
            return (e.text or '').strip().lower() == 'true'
    return False

for group in root.findall('m:ItemGroup', ns):
    for it in group:
        tag = it.tag.split('}')[1]
        inc = it.get('Include')
        if inc is None:
            continue
        if tag == 'ClCompile':
            print('C', 'SKIP' if excluded(it) else 'KEEP', inc)
        elif tag == 'CustomBuild':
            print('ASM', 'SKIP' if excluded(it) else 'KEEP', inc)
        elif tag == 'ResourceCompile':
            print('RC', 'SKIP' if excluded(it) else 'KEEP', inc)
        elif tag == 'None':
            print('NONE', inc)
