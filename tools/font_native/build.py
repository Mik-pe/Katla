#!/usr/bin/env python3
"""Build the source-pinned FreeType/HarfBuzz native C ABI without an engine bridge."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import io
import hashlib
import urllib.parse
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tarfile
import urllib.request

ROOT=Path(__file__).resolve().parents[2]
PINS={"freetype":("freetype/freetype","526ec5c47b9ebccc4754c85ac0c0cdf7c85a5e9b"),"harfbuzz":("harfbuzz/harfbuzz","720f1a3b6699df28c6f600f5d750100ba22d389b"),"sheenbidi":("Tehreer/SheenBidi","cfe430e7375a7845b679adae9d51dac6deaa8858"),"unibreak":("adah1972/libunibreak","304585d8e2d63187507368d612c3d5fff1486368")}

FONT_COMMIT="9710da1eacb3be272583c3224dcb70f9da6eadbb"
FONT_FILES={
    "NotoSansArabic.ttf":("notosansarabic","NotoSansArabic[wdth,wght].ttf"),
    "NotoSansHebrew.ttf":("notosanshebrew","NotoSansHebrew[wdth,wght].ttf"),
    "NotoSansSC.ttf":("notosanssc","NotoSansSC[wght].ttf"),
    "NotoSansDevanagari.ttf":("notosansdevanagari","NotoSansDevanagari[wdth,wght].ttf"),
    "NotoSansThai.ttf":("notosansthai","NotoSansThai[wdth,wght].ttf"),
    "NotoSansSymbols2.ttf":("notosanssymbols2","NotoSansSymbols2-Regular.ttf"),
    "NotoEmoji.ttf":("notoemoji","NotoEmoji[wght].ttf"),
}
FONT_SHA256={'NotoSansArabic.ttf': '63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e', 'NotoSansHebrew.ttf': '7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88', 'NotoSansSC.ttf': 'a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da', 'NotoSansDevanagari.ttf': '14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e', 'NotoSansThai.ttf': '5a1c559bb539583c8a1fd99d1c5b9491e5e14478c9cd2bd0970d5c3096cc9ef8', 'NotoSansSymbols2.ttf': '7d5fb73b7ca67a6798101741f5d280a3d016a56a197afcd4199dbb57b4b82a21', 'NotoEmoji.ttf': 'de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551'}

def font_assets(output):
    folder=output/"fonts";folder.mkdir(parents=True,exist_ok=True)
    manifest={"repository":"google/fonts","commit":FONT_COMMIT,"files":{}}
    for name,(directory,filename) in FONT_FILES.items():
        path=folder/name
        if not path.exists():
            url=f"https://raw.githubusercontent.com/google/fonts/{FONT_COMMIT}/ofl/{directory}/{urllib.parse.quote(filename)}"
            path.write_bytes(urllib.request.urlopen(url,timeout=120).read())
        digest=hashlib.sha256(path.read_bytes()).hexdigest()
        if digest!=FONT_SHA256[name]:raise RuntimeError(f"Pinned font SHA256 mismatch: {path}")
        license_path=folder/(directory+"-OFL.txt")
        if not license_path.exists():license_path.write_bytes(urllib.request.urlopen(f"https://raw.githubusercontent.com/google/fonts/{FONT_COMMIT}/ofl/{directory}/OFL.txt",timeout=120).read())
        manifest["files"][name]=hashlib.sha256(path.read_bytes()).hexdigest()
    (folder/"source-pins.json").write_text(json.dumps(manifest,indent=2)+"\n")

def source(name,folder):
    repo,commit=PINS[name]
    marker=folder/".katla-source-pin"
    if marker.is_file() and marker.read_text().strip()==commit:return
    payload=urllib.request.urlopen(f"https://codeload.github.com/{repo}/tar.gz/{commit}",timeout=120).read()
    with tarfile.open(fileobj=io.BytesIO(payload)) as archive:
        for member in archive.getmembers():
            parts=Path(member.name).parts[1:]
            if not parts or any(part in ("..","") for part in parts):continue
            path=folder.joinpath(*parts)
            if member.isdir():path.mkdir(parents=True,exist_ok=True)
            elif member.isfile():
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_bytes(archive.extractfile(member).read())
    marker.write_text(commit+"\n")

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,default=ROOT/"target/font-native")
    parser.add_argument("--sanitize",action="store_true")
    parser.add_argument("--cc",default=os.environ.get("CC","clang"))
    parser.add_argument("--cxx",default=os.environ.get("CXX","clang++"))
    args=parser.parse_args()
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    font_assets(output)
    sources=ROOT/"target/font-native-sources"
    for name in PINS:source(name,sources/name)
    include=output/"katla_ft_modules.h"
    include.write_text("FT_USE_MODULE(FT_Driver_ClassRec, tt_driver_class)\nFT_USE_MODULE(FT_Module_Class, sfnt_module_class)\nFT_USE_MODULE(FT_Module_Class, psnames_module_class)\nFT_USE_MODULE(FT_Renderer_Class, ft_smooth_renderer_class)\nFT_USE_MODULE(FT_Module_Class, autofit_module_class)\n")
    c=shlex.split(args.cc);cxx=shlex.split(args.cxx)
    flags=["-O2"]
    if platform.system()!="Windows":flags.append("-fPIC")
    if args.sanitize:flags += ["-fsanitize=address","-fno-omit-frame-pointer","-g"]
    ft=sources/"freetype";hb=sources/"harfbuzz"
    ft_files=[f"base/{name}.c" for name in ("ftsystem","ftinit","ftbase","ftdebug","ftglyph","ftbitmap","ftmm","ftbbox","ftfstype","ftgasp","ftstroke","ftsynth")]
    ft_files += ["truetype/truetype.c","sfnt/sfnt.c","smooth/smooth.c","psnames/psnames.c","autofit/autofit.c","gzip/ftgzip.c"]
    objects=[];commands=[]
    for relative in ft_files:
        obj=output/(relative.replace("/","_")+".o");objects.append(obj)
        commands.append([*c,*flags,"-DFT2_BUILD_LIBRARY",'-DFT_CONFIG_MODULES_H="katla_ft_modules.h"',"-I",output,"-I",ft/"include","-c",ft/"src"/relative,"-o",obj])
    obj=output/"harfbuzz.o";objects.append(obj)
    commands.append([*cxx,*flags,"-std=c++17","-DHAVE_FREETYPE","-DHAVE_FT_GET_TRANSFORM","-DHAVE_FT_GET_VAR_BLEND_COORDINATES","-DHAVE_FT_DONE_MM_VAR","-I",ft/"include","-I",hb/"src","-c",hb/"src/harfbuzz.cc","-o",obj])
    bidi=sources/"sheenbidi";brk=sources/"unibreak"
    obj=output/"sheenbidi.o";objects.append(obj)
    commands.append([*c,*flags,"-DSB_CONFIG_UNITY","-I",bidi/"Headers","-I",bidi/"Source","-c",bidi/"Source/SheenBidi.c","-o",obj])
    for name in ("unibreakbase","unibreakdef","linebreak","linebreakdata","linebreakdef","eastasianwidthdef","emojidef","graphemebreak","wordbreak"):
        obj=output/(name+".o");objects.append(obj)
        commands.append([*c,*flags,"-I",brk/"src","-c",brk/"src"/(name+".c"),"-o",obj])
    obj=output/"bridge.o";objects.append(obj)
    commands.append([*cxx,*flags,"-std=c++17","-Wall","-Wextra","-Werror","-I",ft/"include","-I",hb/"src","-I",bidi/"Headers","-I",brk/"src","-c",ROOT/"tools/font_native/bridge.cpp","-o",obj])
    def compile(command):
        subprocess.run(list(map(str,command)),cwd=ROOT,check=True)
    with ThreadPoolExecutor(max_workers=min(8,os.cpu_count() or 1)) as pool:list(pool.map(compile,commands))
    system=platform.system()
    name={"Darwin":"libkatla_font_native.dylib","Linux":"libkatla_font_native.so","Windows":"katla_font_native.dll"}[system]
    command=[*cxx,*flags,"-dynamiclib" if system=="Darwin" else "-shared",*objects,"-o",output/name]
    if system=="Darwin":command += ["-Wl,-install_name,@rpath/"+name]
    subprocess.run(list(map(str,command)),cwd=ROOT,check=True)
    license_output=output/"licenses";license_output.mkdir(exist_ok=True)
    for license_path in (ROOT/"tools/font_native/licenses").iterdir():shutil.copy2(license_path,license_output/license_path.name)
    (output/"source-pins.json").write_text(json.dumps(PINS,indent=2)+"\n")
    print(output/name)

if __name__=="__main__":main()
