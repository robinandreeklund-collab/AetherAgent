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
  // If crab-track exists but has no ferris SVG, inject it
  var track = document.querySelector('.crab-track');
  if(track && !document.getElementById('ferris')){
    track.innerHTML = '<svg class="crab" id="ferris" viewBox="0 0 16 14" xmlns="http://www.w3.org/2000/svg" fill="#f97316"><g class="claw-l"><rect x="1" y="0" width="2" height="2"/><rect x="0" y="1" width="1" height="2"/></g><g class="claw-r"><rect x="13" y="0" width="2" height="2"/><rect x="15" y="1" width="1" height="2"/></g><rect x="2" y="2" width="1" height="3"/><rect x="13" y="2" width="1" height="3"/><rect x="4" y="2" width="8" height="7" rx="1"/><rect x="6" y="4" width="1" height="1" fill="#0a0a0a"/><rect x="9" y="4" width="1" height="1" fill="#0a0a0a"/><rect x="6" y="6" width="4" height="1" fill="#0a0a0a"/><rect x="7" y="7" width="2" height="1" fill="#0a0a0a"/><rect x="3" y="9" width="1" height="2"/><rect x="5" y="9" width="1" height="3"/><rect x="10" y="9" width="1" height="3"/><rect x="12" y="9" width="1" height="2"/><rect x="2" y="11" width="2" height="1"/><rect x="4" y="12" width="2" height="1"/><rect x="10" y="12" width="2" height="1"/><rect x="12" y="11" width="2" height="1"/></svg>';
  }
  var el=document.getElementById('ferris');
  if(!el)return;
  var track=el.parentElement,x=-40,dir=1,speed=.4;
  var nextAction=performance.now()+2e3; // First action after 2s
  var state='walking',stateUntil=0,activeNode=null,jumpStart=0;

  // Ensure smooth bottom transitions
  el.style.transition='transform .3s ease';

  // Reset crab to ground level
  function resetBottom(){el.style.bottom='0px';}
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
    var lx = x - (dir * 10);
    if(lx < 0 || lx > track.offsetWidth) return;
    var line=document.createElement('span');
    line.style.cssText='position:absolute;left:'+lx+'px;bottom:'+(6+Math.random()*14)+'px;width:'+(8+Math.random()*12)+'px;height:1.5px;background:rgba(59,130,246,0.3);pointer-events:none;border-radius:1px;animation:speedFade .4s ease forwards';
    track.appendChild(line);
    setTimeout(function(){line.remove()},400);
  }

  // Inject keyframes if not already present
  if(!document.getElementById('ferris-extra-css')){
    var style=document.createElement('style');
    style.id='ferris-extra-css';
    style.textContent='@keyframes zzFloat{0%{opacity:1;transform:translateY(0)}100%{opacity:0;transform:translateY(-18px) translateX(6px)}}@keyframes speedFade{0%{opacity:.5;transform:scaleX(1)}100%{opacity:0;transform:scaleX(0.2)}}@keyframes tokenFloat{0%{opacity:1;transform:translateY(0) scale(1)}100%{opacity:0;transform:translateY(-22px) scale(1.3)}}@keyframes nomPop{0%{opacity:1;transform:scale(0.5)}40%{transform:scale(1.2)}100%{opacity:0;transform:scale(0.8) translateY(-10px)}}@keyframes starPop{0%{opacity:1;transform:scale(0) rotate(0)}50%{opacity:1;transform:scale(1.2) rotate(180deg)}100%{opacity:0;transform:scale(0.5) rotate(360deg) translateY(-15px)}}@keyframes dirtPop{0%{opacity:.6;transform:translate(0,0)}100%{opacity:0;transform:translate(var(--dx),var(--dy))}}@keyframes surfBob{0%,100%{transform:translateY(0) rotate(-2deg)}50%{transform:translateY(-3px) rotate(2deg)}}';
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

  // ── VICTORY DANCE — 3 mini hops + star ──
  var victoryHop=0, victoryStart=0;
  function spawnStar(){
    var s=document.createElement('span');
    s.textContent='★';
    s.style.cssText='position:absolute;left:'+(x+12)+'px;bottom:30px;font-size:.7rem;color:#f59e0b;pointer-events:none;animation:starPop .6s ease forwards';
    track.appendChild(s);
    setTimeout(function(){s.remove()},600);
  }

  // ── SCARED — run away from big node ──
  var scaredFrom=0, scaredNode=null;
  var bigDomNodes=['<div class="mega-bundle-39f2a.min.js">','<script src="analytics-tracker.js">','<iframe src="ads.doubleclick.net">'];
  function spawnBigNode(nx){
    var text=bigDomNodes[Math.floor(Math.random()*bigDomNodes.length)];
    var node=document.createElement('span');
    node.className='slash-node falling-in';
    node.textContent=text;
    node.style.left=nx+'px';
    node.style.fontSize='.75rem';
    node.style.padding='.25rem .6rem';
    node.style.borderColor='rgba(239,68,68,0.3)';
    node.style.background='rgba(239,68,68,0.06)';
    track.appendChild(node);
    return{el:node,text:text,x:nx};
  }

  // ── EATING TOKENS ──
  var tokenEl=null, tokensEaten=0;
  function spawnToken(nx){
    var t=document.createElement('span');
    t.textContent='T';
    t.style.cssText='position:absolute;left:'+nx+'px;top:10px;font-family:var(--mono);font-size:.7rem;font-weight:700;color:#f59e0b;pointer-events:none;border:1px solid rgba(245,158,11,0.3);border-radius:50%;width:16px;height:16px;display:flex;align-items:center;justify-content:center;background:rgba(245,158,11,0.08)';
    track.appendChild(t);
    return{el:t,x:nx};
  }
  function eatToken(info){
    if(info.el.parentNode)info.el.remove();
    var nom=document.createElement('span');
    nom.textContent='nom';
    nom.style.cssText='position:absolute;left:'+(x+8)+'px;bottom:28px;font-size:.5rem;color:#f59e0b;font-weight:700;pointer-events:none;animation:nomPop .5s ease forwards';
    track.appendChild(nom);
    setTimeout(function(){nom.remove()},500);
  }

  // ── SURFING ──
  var surfNode=null, surfStart=0;

  // ── DIGGING ──
  var digStart=0, dugNode=null;
  var hiddenNodes=['<script type="tracking">','<pixel src="spy.gif">','<div hidden>'];
  function spawnDirt(){
    for(var i=0;i<5;i++){
      var d=document.createElement('span');
      d.textContent='·';
      var dx=(Math.random()-.5)*20, dy=-(5+Math.random()*12);
      d.style.cssText='position:absolute;left:'+(x+12+Math.random()*8)+'px;bottom:4px;font-size:.5rem;color:rgba(245,158,11,0.5);pointer-events:none;--dx:'+dx+'px;--dy:'+dy+'px;animation:dirtPop .5s ease '+(i*60)+'ms forwards';
      track.appendChild(d);
      setTimeout((function(e){return function(){e.remove()}})(d),600);
    }
  }
  function spawnHiddenNode(){
    var text=hiddenNodes[Math.floor(Math.random()*hiddenNodes.length)];
    var node=document.createElement('span');
    node.className='slash-node';
    node.textContent=text;
    node.style.cssText='position:absolute;left:'+(x+4)+'px;top:12px;opacity:0;font-family:var(--mono);font-size:.6rem;color:rgba(239,68,68,0.5);padding:.15rem .4rem;border:1px dashed rgba(239,68,68,0.3);border-radius:3px;pointer-events:none;transition:opacity .3s';
    track.appendChild(node);
    setTimeout(function(){node.style.opacity='1'},50);
    return{el:node,text:text,x:x+4};
  }

  var sleepZzzTimer=0;
  var peekSide=0;
  var speedLineTimer=0;
  // Dynamic action selection — tracks when each action was last done
  var lastDone={};
  var lastAction='';
  // Force showcase: first 5 actions cycle through new behaviors
  var forceQueue=['eat','scared','surf','dig','hunt'];
  var forceIdx=0;

  function tick(now){
    var w=track.offsetWidth;

    // VICTORY DANCE — 3 mini hops after a slash
    if(state==='victory'){
      var vt=(now-victoryStart)/600;
      if(vt<1){
        var hop=Math.abs(Math.sin(vt*Math.PI*3))*12;
        el.style.bottom=hop+'px';
        el.style.transform='scaleX('+(dir<0?-1:1)+') rotate('+(Math.sin(vt*Math.PI*6)*15)+'deg)';
      }else{
        resetBottom();
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        spawnStar();
        state='walking';nextAction=now+1500+Math.random()*2e3;
      }
      el.style.left=x+'px';requestAnimationFrame(tick);return;
    }

    // SCARED — running away from big node
    if(state==='scared'){
      x+=dir*2.5;
      if(now>speedLineTimer){spawnSpeedLine();speedLineTimer=now+40;}
      if(now>stateUntil){
        dir*=-1;el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        if(scaredNode){activeNode=scaredNode;scaredNode=null;}
        state='hunting';
      }
      if(x>w+40)x=-40;if(x<-40)x=w+40;
      el.style.left=x+'px';requestAnimationFrame(tick);return;
    }

    // EATING — running to token
    if(state==='eating'&&tokenEl){
      var dx2=tokenEl.x-x;
      if(Math.abs(dx2)>5){
        dir=dx2>0?1:-1;
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        x+=dir*1;
      }else{
        eatToken(tokenEl);tokenEl=null;tokensEaten++;
        speed=.5+Math.min(tokensEaten*.05,.3);
        state='walking';nextAction=now+1500+Math.random()*2e3;
      }
      el.style.left=x+'px';requestAnimationFrame(tick);return;
    }

    // SURFING — riding a DOM node
    if(state==='surfing'&&surfNode){
      var st2=(now-surfStart)/2000;
      if(st2<1){
        x+=dir*1.2;
        surfNode.el.style.left=x+'px';
        // Smooth bob — use sin wave, crab sits on top of node
        el.style.bottom=(10+Math.sin(st2*Math.PI*4)*3)+'px';
        el.style.transform='scaleX('+(dir<0?-1:1)+') rotate('+(Math.sin(st2*Math.PI*4)*5)+'deg)';
      }else{
        resetBottom();
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        if(surfNode.el.parentNode){
          surfNode.el.classList.add('slashed-r');
          var sn=surfNode;
          setTimeout(function(){if(sn.el.parentNode)sn.el.remove();},600);
        }
        surfNode=null;
        state='walking';nextAction=now+1500+Math.random()*2e3;
      }
      if(x>w+40)x=-40;if(x<-40)x=w+40;
      el.style.left=x+'px';requestAnimationFrame(tick);return;
    }

    // DIGGING — digging up hidden node
    if(state==='digging'){
      var dt=(now-digStart)/1500;
      if(dt<0.4){
        el.style.transform='scaleX('+(dir<0?-1:1)+') rotate('+(Math.sin(dt*Math.PI*10)*12)+'deg)';
        if(dt>.1&&dt<.12)spawnDirt();
        if(dt>.2&&dt<.22)spawnDirt();
        if(dt>.3&&dt<.32)spawnDirt();
      }else if(dt<0.6){
        if(!dugNode){dugNode=spawnHiddenNode();}
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
      }else if(dt<0.85){
        if(dugNode&&dt>.7&&dt<.72){
          // Jump to slash
          state='jumping';jumpStart=now;activeNode=dugNode;dugNode=null;
        }
      }else{
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        state='walking';nextAction=now+1500+Math.random()*2e3;
        if(dugNode){if(dugNode.el.parentNode)dugNode.el.remove();dugNode=null;}
      }
      el.style.left=x+'px';requestAnimationFrame(tick);return;
    }

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

    // PEEKING — hiding at edge, slowly peek out
    if(state==='peeking'){
      if(now>stateUntil){
        // Slide back in smoothly
        state='walking';
        el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
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
        resetBottom();
        if(Math.random()<.4){state='victory';victoryStart=now;}
        else{state='walking';nextAction=now+1500+Math.random()*2e3;}
      }
      el.style.left=x+'px';requestAnimationFrame(tick);return;
    }

    // WALKING — normal movement + decisions
    if(state==='walking'){
      x+=speed*dir;
      if(now>nextAction){
        // Dynamic probability — boost actions not done recently
        var actions=['turn','pause','hunt','sleep','peek','scared','eat','surf','dig'];
        // Weight: base + recency bonus (higher = longer since last done)
        var weights=[8,5,15,6,4,10,10,10,10];
        for(var ai=0;ai<actions.length;ai++){
          var since=now-(lastDone[actions[ai]]||0);
          // Boost by 1 point per 3 seconds since last done (max +15)
          weights[ai]+=Math.min(Math.floor(since/3000),15);
          // Never repeat same action twice
          if(actions[ai]===lastAction) weights[ai]=0;
          // Peek only near edge
          if(actions[ai]==='peek'&&x>30&&x<w-50) weights[ai]=0;
        }
        var chosen;
        if(forceIdx<forceQueue.length){
          chosen=forceQueue[forceIdx];forceIdx++;
          console.log('[FERRIS] FORCED action: '+chosen+' ('+forceIdx+'/'+forceQueue.length+')');
        }else{
          var totalW=weights.reduce(function(a,b){return a+b},0);
          var r=Math.random()*totalW, cum=0; chosen='turn';
          for(var ai2=0;ai2<actions.length;ai2++){
            cum+=weights[ai2];
            if(r<cum){chosen=actions[ai2];break;}
          }
        }
        lastAction=chosen;
        lastDone[chosen]=now;

        if(chosen==='turn'){
          console.log('[FERRIS] turn');
          dir*=-1;el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
        } else if(chosen==='pause'){
          console.log('[FERRIS] pause');
          state='paused';stateUntil=now+400+Math.random()*1e3;
        } else if(chosen==='hunt'){
          console.log('[FERRIS] hunt');
          var nx=40+Math.random()*(w-80);
          activeNode=spawnNodeAt(nx);state='hunting';
        } else if(chosen==='sleep'){
          console.log('[FERRIS] sleep');
          state='sleeping';stateUntil=now+2500+Math.random()*2e3;
          sleepZzzTimer=now+300;
        } else if(chosen==='peek'){
          console.log('[FERRIS] peek');
          state='peeking';stateUntil=now+1500+Math.random()*1500;
          if(x<30){el.style.transform='scaleX(1) rotate(-12deg)';dir=1;}
          else{el.style.transform='scaleX(-1) rotate(12deg)';dir=-1;}
        } else if(chosen==='scared'){
          console.log('[FERRIS] scared at x='+x+' w='+w);
          var bnx=40+Math.random()*(w-80);
          scaredNode=spawnBigNode(bnx);
          dir=x<bnx?-1:1;
          el.style.transform=dir<0?'scaleX(-1)':'scaleX(1)';
          state='scared';stateUntil=now+1200+Math.random()*800;
        } else if(chosen==='eat'){
          console.log('[FERRIS] eat');
          var tnx=40+Math.random()*(w-80);
          tokenEl=spawnToken(tnx);
          state='eating';
        } else if(chosen==='surf'){
          console.log('[FERRIS] surf');
          var snx=x+dir*10;
          surfNode=spawnNodeAt(snx);
          surfStart=now;
          state='surfing';
        } else if(chosen==='dig'){
          console.log('[FERRIS] dig');
          state='digging';digStart=now;dugNode=null;
        }
        speed=.3+Math.random()*.35;
        nextAction=now+1500+Math.random()*2500;
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
