// sl/sh shared components — header, footer, Ferris crab
// Include on every page: <script src="/components.js"></script>

(function(){
  // ── NAV ──
  const nav = document.querySelector('nav[data-component="nav"]');
  if(nav){
    const current = nav.dataset.current || '';
    nav.innerHTML = `
      <a href="/" class="wordmark">sl<span class="wm-slash">/</span>sh</a>
      <div class="nav-links">
        <a href="/#features">Features</a>
        <a href="/mission"${current==='mission'?' class="active"':''}>Mission</a>
        <a href="/timeline"${current==='timeline'?' class="active"':''}>Timeline</a>
        <a href="/live"${current==='live'?' class="active"':''}>Live</a>
        <a href="/docs"${current==='docs'?' class="active"':''}>Docs</a>
        <a class="btn-sm btn-accent" href="/try">Try alpha</a>
      </div>
    `;
  }

  // ── FOOTER ──
  const footerSlot = document.querySelector('[data-component="footer"]');
  if(footerSlot){
    footerSlot.outerHTML = `
<div class="crab-track">
  <svg class="crab" id="ferris" viewBox="0 0 16 14" xmlns="http://www.w3.org/2000/svg" fill="#f97316">
    <g class="claw-l"><rect x="1" y="0" width="2" height="2"/><rect x="0" y="1" width="1" height="2"/></g>
    <g class="claw-r"><rect x="13" y="0" width="2" height="2"/><rect x="15" y="1" width="1" height="2"/></g>
    <rect x="2" y="2" width="1" height="3"/><rect x="13" y="2" width="1" height="3"/>
    <rect x="4" y="2" width="8" height="7" rx="1"/>
    <rect x="6" y="4" width="1" height="1" fill="#0a0a0a"/><rect x="9" y="4" width="1" height="1" fill="#0a0a0a"/>
    <rect x="6" y="6" width="4" height="1" fill="#0a0a0a"/><rect x="7" y="7" width="2" height="1" fill="#0a0a0a"/>
    <rect x="3" y="9" width="1" height="2"/><rect x="5" y="9" width="1" height="3"/>
    <rect x="10" y="9" width="1" height="3"/><rect x="12" y="9" width="1" height="2"/>
    <rect x="2" y="11" width="2" height="1"/><rect x="4" y="12" width="2" height="1"/>
    <rect x="10" y="12" width="2" height="1"/><rect x="12" y="11" width="2" height="1"/>
  </svg>
</div>
<footer>
  <div class="footer-glow"></div>
  <div class="footer-inner">
    <div class="footer-left">
      <div class="tagline">Kill the noise. Find the signal.</div>
      <a href="mailto:zerotoken@slaash.ai" style="font-size:.7rem;color:#444;transition:color .25s">zerotoken@slaash.ai</a>
    </div>
    <div class="footer-center">
      <div class="footer-logo">sl<span class="fl-slash">/</span>sh</div>
      <div class="footer-year">&copy; 2026</div>
    </div>
    <div class="footer-right">
      <a href="/mission">Mission</a>
      <a href="/timeline">Timeline</a>
      <a href="/live">Live</a>
      <a href="/try">Try alpha</a>
    </div>
  </div>
  <div class="footer-bottom">Built in Rust. No GPU. No model files. Just signal.</div>
</footer>`;
  }

  // ── FERRIS CRAB ──
  var el=document.getElementById('ferris');
  if(!el)return;
  var track=el.parentElement,x=-40,dir=1,speed=.4;
  var nextAction=performance.now()+3e3+Math.random()*4e3;
  var state='walking',stateUntil=0,activeNode=null,jumpStart=0;
  var domNodes=['<div>','<nav>','<span>','<meta>','<script>','<iframe>',
    '<footer>','<aside>','style=""','onclick=""','tracking.js',
    'ads.min.js','<table>','display:none'];

  function spawnNodeAt(nx){
    var text=domNodes[Math.floor(Math.random()*domNodes.length)];
    var node=document.createElement('span');
    node.className='slash-node falling-in';
    node.textContent=text;
    node.style.left=nx+'px';
    track.appendChild(node);
    return{el:node,text:text,x:nx};
  }

  function slashNode(info){
    if(info.el.parentNode)info.el.remove();
    var half=Math.ceil(info.text.length/2);
    var fL=document.createElement('span');
    fL.className='slash-node slashed-l';
    fL.textContent=info.text.slice(0,half);
    fL.style.left=info.x+'px';
    fL.style.top='8px';
    track.appendChild(fL);
    var fR=document.createElement('span');
    fR.className='slash-node slashed-r';
    fR.textContent=info.text.slice(half);
    fR.style.left=(info.x+20)+'px';
    fR.style.top='8px';
    track.appendChild(fR);
    setTimeout(function(){fL.remove();fR.remove()},700);
  }

  function tick(now){
    var w=track.offsetWidth;
    if(state==='paused'){
      if(now>stateUntil)state='walking';
      requestAnimationFrame(tick);return;
    }
    if(state==='hunting'&&activeNode){
      var dx=activeNode.x-x;
      if(Math.abs(dx)>5){
        dir=dx>0?1:-1;
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        x+=dir*1.2;
      }else{state='jumping';jumpStart=now;}
    }
    if(state==='jumping'){
      var t=(now-jumpStart)/350;
      if(t<1){
        el.style.bottom=(Math.sin(t*Math.PI)*22)+'px';
        if(t>.4&&t<.5&&activeNode){slashNode(activeNode);activeNode=null;}
      }else{
        el.style.bottom='0px';state='walking';
        nextAction=now+2e3+Math.random()*3e3;
      }
    }
    if(state==='walking'){
      x+=speed*dir;
      if(now>nextAction){
        var roll=Math.random();
        if(roll<.2){dir*=-1;el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';}
        else if(roll<.35){state='paused';stateUntil=now+400+Math.random()*1e3;}
        else if(roll<.55){var nx=40+Math.random()*(w-80);activeNode=spawnNodeAt(nx);state='hunting';}
        speed=.3+Math.random()*.35;
        nextAction=now+2500+Math.random()*4e3;
      }
    }
    if(x>w+40)x=-40;if(x<-40)x=w+40;
    el.style.left=x+'px';
    requestAnimationFrame(tick);
  }
  requestAnimationFrame(tick);
})();
