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

  // Spawn ZzZ sleep bubbles
  function spawnZzz(){
    var z=document.createElement('span');
    z.textContent='z';
    z.style.cssText='position:absolute;left:'+(x+14)+'px;bottom:28px;font-size:.5rem;color:rgba(255,255,255,0.25);pointer-events:none;animation:zzFloat 1.5s ease forwards';
    track.appendChild(z);
    setTimeout(function(){
      var z2=document.createElement('span');
      z2.textContent='Z';
      z2.style.cssText='position:absolute;left:'+(x+18)+'px;bottom:32px;font-size:.6rem;color:rgba(255,255,255,0.3);pointer-events:none;animation:zzFloat 1.5s ease .3s forwards;opacity:0';
      track.appendChild(z2);
      setTimeout(function(){z2.remove()},1800);
    },400);
    setTimeout(function(){z.remove()},1500);
  }

  // Spawn speed lines behind crab
  function spawnSpeedLine(){
    var line=document.createElement('span');
    line.style.cssText='position:absolute;left:'+(x-(dir*8))+'px;bottom:'+(4+Math.random()*16)+'px;width:'+(6+Math.random()*10)+'px;height:1px;background:rgba(255,255,255,0.15);pointer-events:none;animation:speedFade .3s ease forwards';
    track.appendChild(line);
    setTimeout(function(){line.remove()},300);
  }

  // Inject keyframes if not already present
  if(!document.getElementById('ferris-extra-css')){
    var style=document.createElement('style');
    style.id='ferris-extra-css';
    style.textContent='@keyframes zzFloat{0%{opacity:1;transform:translateY(0)}100%{opacity:0;transform:translateY(-18px) translateX(6px)}}@keyframes speedFade{0%{opacity:.3;transform:scaleX(1)}100%{opacity:0;transform:scaleX(0.3)}}';
    document.head.appendChild(style);
  }

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

  var sleepZzzTimer=0;
  var peekSide=0; // 0=not peeking, -1=left edge, 1=right edge
  var speedLineTimer=0;

  function tick(now){
    var w=track.offsetWidth;

    // SLEEPING — sit still, spawn ZzZ bubbles
    if(state==='sleeping'){
      if(now>stateUntil){
        state='walking';
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
      } else if(now>sleepZzzTimer){
        spawnZzz();
        sleepZzzTimer=now+800+Math.random()*600;
      }
      requestAnimationFrame(tick);return;
    }

    // PEEKING — hide at edge, only eyes visible
    if(state==='peeking'){
      if(now>stateUntil){
        state='walking';
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        x=peekSide<0?4:w-36;
      }
      requestAnimationFrame(tick);return;
    }

    // PAUSED
    if(state==='paused'){
      if(now>stateUntil)state='walking';
      requestAnimationFrame(tick);return;
    }

    // HUNTING — run to node with speed lines
    if(state==='hunting'&&activeNode){
      var dx=activeNode.x-x;
      if(Math.abs(dx)>5){
        dir=dx>0?1:-1;
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        x+=dir*1.5;
        if(now>speedLineTimer){spawnSpeedLine();speedLineTimer=now+60;}
      }else{state='jumping';jumpStart=now;}
    }

    // JUMPING
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

    // WALKING — normal movement + decisions
    if(state==='walking'){
      x+=speed*dir;
      if(now>nextAction){
        var roll=Math.random();
        if(roll<.15){
          // Turn around
          dir*=-1;el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        } else if(roll<.25){
          // Pause
          state='paused';stateUntil=now+400+Math.random()*1e3;
        } else if(roll<.42){
          // Hunt a DOM node!
          var nx=40+Math.random()*(w-80);
          activeNode=spawnNodeAt(nx);state='hunting';
        } else if(roll<.52){
          // Fall asleep ZzZ
          state='sleeping';stateUntil=now+2500+Math.random()*2e3;
          sleepZzzTimer=now+300;
        } else if(roll<.60){
          // Peek from edge
          state='peeking';stateUntil=now+1500+Math.random()*1500;
          peekSide=Math.random()<.5?-1:1;
          x=peekSide<0?-20:w-12;
          el.style.left=x+'px';
          el.style.transform=peekSide<0?'scaleX(1) rotate(-15deg)':'scaleX(-1) rotate(15deg)';
        }
        speed=.3+Math.random()*.35;
        nextAction=now+2500+Math.random()*4e3;
      }
    }

    // Wrap edges (only when walking)
    if(state==='walking'||state==='hunting'){
      if(x>w+40)x=-40;if(x<-40)x=w+40;
    }
    el.style.left=x+'px';
    requestAnimationFrame(tick);
  }
  requestAnimationFrame(tick);
})();
