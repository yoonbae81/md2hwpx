#!/usr/bin/env python3
"""web/public/template.hwpx에 [[table:2]] MEMO 표식 문단과 네이티브 2열
table-2 tbl 원형(hp assets/block/table-2.hwpx에서 운반)을 추가한다.

- 참조 닫기: 가져온 tbl이 참조하는 borderFill/charPr/paraPr(+tabPr) 정의를
  legacy header의 새 id 공간으로 재번호해 복사한다. 동일 내용 정의가 legacy에
  이미 있으면 복사 없이 그 id를 재사용한다.
- charPr의 fontRef 언어별 font id는 hp2의 lang+facename을 legacy fontfaces에서
  같은 lang·같은 facename인 id로 사상한다. 없는 면이면 명시적으로 실패한다.
- zip 재작성 시 entry 순서와 compress_type을 보존한다(mimetype은 무압축 첫
  entry). hp 저장소는 읽기만 한다.

검증에 실패하면 비영 종료한다.
"""
import re
import sys
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
HP2 = Path("/Users/y/Github/hp/assets/block/table-2.hwpx")
TPL = REPO / "web/public/template.hwpx"

LANGS = ["hangul", "latin", "hanja", "japanese", "other", "symbol", "user"]
CONTAINERS = {
    "borderFill": "borderFills",
    "charPr": "charProperties",
    "paraPr": "paraProperties",
    "tabPr": "tabProperties",
}


def fail(msg):
    print("script-check FAIL:", msg)
    sys.exit(1)


def norm(def_xml):
    """정의 비교용: id 속성 제거 + 공백 축소."""
    s = re.sub(r'\s+id="\d+"', "", def_xml)
    return re.sub(r"\s+", "", s)


def extract_defs(header, tag):
    """header에서 tag 정의 목록: (id, 원문, norm)."""
    out = []
    for m in re.finditer(r"<hh:%s .*?</hh:%s>" % (tag, tag), header, re.S):
        xml = m.group(0)
        idm = re.search(r' id="(\d+)"', xml)
        out.append((idm.group(1), xml, norm(xml)))
    return out


def get_def(defs, tag, id_):
    for i, xml, _ in defs[tag]:
        if i == id_:
            return xml
    fail("%s id=%s not found in hp2 header" % (tag, id_))


def fontfaces(header):
    """{lang_lower: {font_id: facename}}"""
    out = {}
    for m in re.finditer(
        r'<hh:fontface lang="(\w+)" fontCnt="\d+">(.*?)</hh:fontface>', header, re.S
    ):
        out[m.group(1).lower()] = dict(
            (i, face) for i, face in re.findall(r'<hh:font id="(\d+)" face="([^"]*)"', m.group(2))
        )
    return out


def main():
    hp2_bytes = HP2.read_bytes()
    z2 = zipfile.ZipFile(HP2)
    s2 = z2.read("Contents/section0.xml").decode("utf8")
    h2 = z2.read("Contents/header.xml").decode("utf8")

    with zipfile.ZipFile(TPL) as z:
        names = z.namelist()
        infos = z.infolist()
        contents = {n: z.read(n) for n in names}
        assert names[0] == "mimetype" and infos[0].compress_type == 0

    sec = contents["Contents/section0.xml"].decode("utf8")
    hdr = contents["Contents/header.xml"].decode("utf8")

    if "[[table:2]]" in sec:
        fail("template already contains [[table:2]]")

    # --- hp2 tbl 추출 ---
    j = s2.find("<hp:tbl")
    k = s2.find("</hp:tbl>") + len("</hp:tbl>")
    if j < 0 or k < len("</hp:tbl>"):
        fail("no tbl in hp2 section0")
    tbl = s2[j:k]

    hp2_defs = {t: extract_defs(h2, t) for t in CONTAINERS}
    legacy_defs = {t: extract_defs(hdr, t) for t in CONTAINERS}

    # --- 참조 닫기 계산 ---
    wanted = {t: set() for t in CONTAINERS}  # hp2 ids needed
    for tag in ["borderFillIDRef", "charPrIDRef", "paraPrIDRef", "tabPrIDRef"]:
        kind = tag[: -len("IDRef")]
        wanted[kind].update(re.findall(r'%s="(\d+)"' % tag, tbl))
    # charPr → borderFill, fontRef closure
    queue = list(wanted["charPr"])
    while queue:
        cp = get_def(hp2_defs, "charPr", queue.pop())
        for bf in re.findall(r'borderFillIDRef="(\d+)"', cp):
            wanted["borderFill"].add(bf)
    for pp in list(wanted["paraPr"]):
        px = get_def(hp2_defs, "paraPr", pp)
        for tp in re.findall(r'tabPrIDRef="(\d+)"', px):
            wanted["tabPr"].add(tp)

    if "0" not in {i for i, _, _ in legacy_defs["tabPr"]}:
        pass  # tabPr 0은 스타일처럼 기본 id; 아래 사상에서 처리된다.

    # styleIDRef="0"은 legacy id 0 그대로 사용 — 존재 확인.
    if 'id="0"' not in re.search(r"<hh:styles.*?</hh:styles>", hdr, re.S).group(0):
        fail("legacy styles has no id 0")

    # --- font 사상 (lang + facename) ---
    hp2_fonts = fontfaces(h2)
    legacy_fonts = fontfaces(hdr)
    font_map = {}  # (lang, hp2 font id) -> legacy font id
    for lang in LANGS:
        fontref_ids = set()
        for i in wanted["charPr"]:
            cp = get_def(hp2_defs, "charPr", i)
            fr = re.search(r"<hh:fontRef [^>]*/>", cp)
            if fr:
                fontref_ids.update(re.findall(r'%s="(\d+)"' % lang, fr.group(0)))
        for fid in fontref_ids:
            face = hp2_fonts.get(lang, {}).get(fid)
            if face is None:
                fail("hp2 %s font %s not found" % (lang, fid))
            inv = {f: i for i, f in legacy_fonts.get(lang, {}).items()}
            if face not in inv:
                fail("legacy %s fontface has no face %r" % (lang, face))
            font_map[(lang, fid)] = inv[face]

    # --- id 재번호 사상 (동일 내용은 재사용) ---
    id_maps = {t: {} for t in CONTAINERS}
    copied = {t: [] for t in CONTAINERS}
    for tag in CONTAINERS:
        for id_ in sorted(wanted[tag], key=int):
            src = get_def(hp2_defs, tag, id_)
            n = norm(src)
            reuse = next((i for i, _, nn in legacy_defs[tag] if nn == n), None)
            if reuse is not None:
                id_maps[tag][id_] = reuse
                continue
            # tabPr 0(빈 기본값) 같은 항목은 동일 내용이 있어야 재사용된다.
            new_id = str(max(int(i) for i, _, _ in legacy_defs[tag]) + 1)
            new_xml = re.sub(r' id="\d+"', ' id="%s"' % new_id, src, count=1)
            legacy_defs[tag].append((new_id, new_xml, n))
            copied[tag].append((id_, new_id))
            id_maps[tag][id_] = new_id

    # --- tbl 참조 재번호 ---
    def sub_attr(xml, attr, mapper):
        return re.sub(
            r'%s="(\d+)"' % attr,
            lambda m: '%s="%s"' % (attr, mapper(m.group(1))),
            xml,
        )

    new_tbl = tbl
    new_tbl = sub_attr(new_tbl, "borderFillIDRef", lambda v: id_maps["borderFill"].get(v, v))
    new_tbl = sub_attr(new_tbl, "charPrIDRef", lambda v: id_maps["charPr"].get(v, v))
    new_tbl = sub_attr(new_tbl, "paraPrIDRef", lambda v: id_maps["paraPr"].get(v, v))
    # styleIDRef는 사상하지 않는다(0 그대로).
    # 복사된 charPr 내부 borderFillIDRef와 fontRef 재번호.
    fixed_copied = {t: [] for t in CONTAINERS}
    for tag, pairs in copied.items():
        for old, new in pairs:
            xml = next(x for i, x, _ in legacy_defs[tag] if i == new)
            if tag == "charPr":
                xml = sub_attr(xml, "borderFillIDRef", lambda v: id_maps["borderFill"].get(v, v))
                m = re.search(r"<hh:fontRef [^>]*/>", xml)
                if m:
                    fr = m.group(0)
                    for lang in LANGS:
                        def repl(mo, lang=lang):
                            key = (lang, mo.group(1))
                            if key in font_map:
                                return '%s="%s"' % (lang, font_map[key])
                            return mo.group(0)
                        fr = re.sub(r'%s="(\d+)"' % lang, repl, fr)
                    xml = xml.replace(m.group(0), fr)
            if tag == "paraPr":
                xml = sub_attr(xml, "tabPrIDRef", lambda v: id_maps["tabPr"].get(v, v))
            fixed_copied[tag].append((new, xml))

    # --- header에 삽입 + itemCnt 갱신 ---
    new_hdr = hdr
    for tag, cont in CONTAINERS.items():
        if not fixed_copied[tag]:
            continue
        close = "</hh:%s>" % cont
        if close not in new_hdr:
            fail("legacy header has no </hh:%s>" % cont)
        insert = "".join(x for _, x in fixed_copied[tag])
        new_hdr = new_hdr.replace(close, insert + close, 1)
        m = re.search(r'<hh:%s [^>]*itemCnt="(\d+)"' % cont, new_hdr)
        if not m:
            fail("no itemCnt on hh:%s" % cont)
        new_hdr = new_hdr[: m.start()] + m.group(0).replace(
            'itemCnt="%s"' % m.group(1), 'itemCnt="%d"' % (int(m.group(1)) + len(fixed_copied[tag]))
        ) + new_hdr[m.end():]

    # --- section0: 3열 표 블록 뒤에 [[table:2]] 문단 + tbl 문단 추가 ---
    i3 = sec.find("[[table]]")
    if i3 < 0:
        fail("no [[table]] marker in legacy section0")
    # tbl 문단(p2): tbl이 속한 최상위 문단 (먼저 구해 fieldEnd beginIDRef를 얻는다)
    t3 = sec.find("<hp:tbl", i3)
    p2_start = sec.rfind("<hp:p ", 0, t3)
    depth, pos = 0, p2_start
    while True:
        mo = re.compile(r"<hp:p[ >]|</hp:p>").search(sec, pos)
        if not mo:
            fail("unbalanced hp:p")
        if mo.group(0).startswith("</"):
            depth -= 1
            if depth == 0:
                p2_end = mo.end()
                break
        else:
            depth += 1
        pos = mo.end()
    p2 = sec[p2_start:p2_end]

    # 표식 문단(p1): p2의 fieldEnd beginIDRef로 fieldBegin을 찾아 그것을
    # 감싸는 최상위 문단 ([[table]] 텍스트는 fieldBegin의 중첩 subList 안에
    # 있다.)
    fe = re.search(r'<hp:fieldEnd beginIDRef="(\d+)" fieldid="(\d+)"', p2)
    if not fe:
        fail("fieldEnd not found in the [[table]] tbl paragraph")
    old_begin_id, old_fieldid = fe.group(1), fe.group(2)
    fb = sec.find('<hp:fieldBegin id="%s"' % old_begin_id)
    if fb < 0 or fb > p2_start:
        fail("fieldBegin for the [[table]] memo not found before the tbl paragraph")
    p1_start = sec.rfind("<hp:p ", 0, fb)
    depth, pos = 0, p1_start
    while True:
        mo = re.compile(r"<hp:p[ >]|</hp:p>").search(sec, pos)
        if not mo:
            fail("unbalanced hp:p around marker")
        if mo.group(0).startswith("</"):
            depth -= 1
            if depth == 0:
                p1_end = mo.end()
                break
        else:
            depth += 1
        pos = mo.end()
    p1 = sec[p1_start:p1_end]

    new_begin_id = "1176938600"
    new_fieldid = "623209900"
    new_tbl_id = "1176938601"
    max_pid = max(int(x) for x in re.findall(r'<hp:p id="(\d+)"', sec) if int(x) < 2000000000)
    new_p1_id = str(max_pid + 1)
    new_p2_id = str(max_pid + 2)

    marker_p = p1
    marker_p = marker_p.replace("[[table]]", "[[table:2]]")
    marker_p = re.sub(r'fieldBegin id="\d+"', 'fieldBegin id="%s"' % new_begin_id, marker_p)
    marker_p = re.sub(r'fieldid="\d+"', 'fieldid="%s"' % new_fieldid, marker_p)
    marker_p = re.sub(r'<hp:p id="\d+"', '<hp:p id="%s"' % new_p1_id, marker_p, count=1)

    new_tbl = re.sub(r'(<hp:tbl [^>]*?\bid=")\d+"', r'\g<1>%s"' % new_tbl_id, new_tbl, count=1)
    table_p = re.sub(r"<hp:tbl .*</hp:tbl>", lambda m: new_tbl, p2, count=1, flags=re.S)
    table_p = re.sub(r'fieldEnd beginIDRef="\d+"', 'fieldEnd beginIDRef="%s"' % new_begin_id, table_p)
    table_p = re.sub(r'fieldid="\d+"', 'fieldid="%s"' % new_fieldid, table_p)
    table_p = re.sub(r'<hp:p id="\d+"', '<hp:p id="%s"' % new_p2_id, table_p, count=1)

    if "[[table:2]]" not in marker_p or new_tbl not in table_p:
        fail("paragraph construction failed")

    new_sec = sec[:p2_end] + marker_p + table_p + sec[p2_end:]

    # --- 자체 검증 ---
    for ref in set(re.findall(r'(?:borderFill|charPr|paraPr|tabPr)IDRef="(\d+)"', new_tbl)):
        kind = next(
            t for t in CONTAINERS
            if re.search(r'<hh:%s [^>]*id="%s"' % (t, ref), new_hdr)
        ) if any(re.search(r'<hh:%s [^>]*id="%s"' % (t, ref), new_hdr) for t in CONTAINERS) else None
        if kind is None:
            fail("new tbl references missing header def id=%s" % ref)
    for run in re.findall(r'charPrIDRef="(\d+)"', new_tbl):
        if not re.search(r'<hh:charPr [^>]*id="%s"' % run, new_hdr):
            fail("new tbl charPrIDRef %s missing" % run)
    for pr in re.findall(r'paraPrIDRef="(\d+)"', new_tbl):
        if not re.search(r'<hh:paraPr [^>]*id="%s"' % pr, new_hdr):
            fail("new tbl paraPrIDRef %s missing" % pr)
    for st in re.findall(r'styleIDRef="(\d+)"', new_tbl):
        if not re.search(r'<hh:style id="%s"' % st, new_hdr):
            fail("new tbl styleIDRef %s missing" % st)
    if new_sec.count("[[table:2]]") != 1:
        fail("[[table:2]] marker count != 1")
    if not re.search(r'fieldBegin id="%s"[^>]*fieldid="%s"' % (new_begin_id, new_fieldid), new_sec):
        fail("new fieldBegin not found")
    if not re.search(r'fieldEnd beginIDRef="%s" fieldid="%s"' % (new_begin_id, new_fieldid), new_sec):
        fail("new fieldEnd not paired")

    # --- zip 재작성 (순서·compress_type 보존) ---
    contents["Contents/section0.xml"] = new_sec.encode("utf8")
    contents["Contents/header.xml"] = new_hdr.encode("utf8")
    tmp = TPL.with_suffix(".hwpx.tmp")
    with zipfile.ZipFile(tmp, "w") as out:
        for info in infos:
            data = contents[info.filename]
            zi = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            zi.compress_type = info.compress_type
            zi.external_attr = info.external_attr
            out.writestr(zi, data)
    tmp.replace(TPL)

    if HP2.read_bytes() != hp2_bytes:
        fail("hp repository file changed")
    print("script-check OK: copied defs =", {t: sorted(p) for t, p in copied.items()})
    print("id_maps =", {t: dict(sorted(id_maps[t].items(), key=lambda kv: int(kv[0]))) for t in CONTAINERS})
    print("font_map =", {f"{l}:{k}": v for (l, k), v in sorted(font_map.items())})


if __name__ == "__main__":
    main()
