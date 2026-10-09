"""Invented EN pages; no text or token spelling copied from production inputs."""


def face(number: str, *, back: bool = False) -> str:
    name = "Synthetic back" if back else "Synthetic front"
    return f"""<div class="cardlist-Detail_Box_Inner">
<h1 class="ttl">{name}</h1><div class="img"><img src="../exact/{number}.png?v=7"></div>
<div class="info"><dl><dt>Class</dt><dd>Synthetic class</dd></dl>
<dl><dt>Card Type</dt><dd>Synthetic type</dd></dl><dl><dt>Trait</dt><dd>Alpha / Beta</dd></dl>
<dl><dt>Rarity</dt><dd>LG</dd></dl><dl><dt>Card Set</dt><dd>Set<br>Edition</dd></dl>
<dl><dt>Universe</dt><dd>Synthetic universe</dd></dl></div>
<div class="status-Item status-Item-Cost"><span class="heading">Cost</span>-</div>
<div class="status-Item status-Item-Power"><span class="heading">Power</span>02</div>
<div class="status-Item status-Item-Hp"><span class="heading">HP</span>X</div>
<div class="detail"><p>First <img src="/icons/synthetic.badge.png?v=1" alt="[badge]"></p>
<p>Next<br>-----<br>Auxiliary<br>------<br>Last</p></div>
<div class="speech">Voice <img src="https://example.invalid/icons/synthetic.voice.svg?x=1" alt="voice"></div>
<div class="illustrator"><span class="heading">Synthetic artist</span><span class="name">{number}</span></div>
</div>"""


def page(number: str = "SYNⓈ-01aEN", *, double: bool = False) -> bytes:
    faces = face(number) + (face(number, back=True) if double else "")
    return (
        '<html><div class="cardlist-Detail">'
        + faces
        + '<div class="illustrator"><a href="/errata/exact?x=1">Notice</a></div></div>'
        + '<div class="cardlist-Under"><div class="cardlist-Detail_Products">'
        + '<div class="cardlist-Detail_Products_Inner"><h2 class="ttl">Synthetic product</h2>'
        + '<div class="date">Synthetic release date</div><a href="/products/exact">Link</a></div></div>'
        + '<div class="cardlist-Detail_QA"><div class="qa-List_Item">'
        + '<div class="qa-List_Ttl">Synthetic QA title</div>'
        + '<div class="qa-List_Txt-Q"><span class="Garamond">Q</span>Question</div>'
        + '<div class="qa-List_Txt-A"><span class="Garamond">A</span>Answer<br>More</div>'
        + '</div></div></div><div class="cardlist-Detail_Relation">'
        + '<a href="/cards/?cardno=EXACTaEN">Related</a></div><!--'
        + "x" * 1100
        + "--></html>"
    ).encode()
