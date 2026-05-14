(function(){
  var canvas = document.getElementById('plasma-canvas');
  if(!canvas) return;

  var gl = canvas.getContext('webgl2') || canvas.getContext('webgl');
  if(!gl) return;

  function resize(){
    var dpr = window.devicePixelRatio || 1;
    canvas.width = window.innerWidth * dpr;
    canvas.height = window.innerHeight * dpr;
    gl.viewport(0, 0, canvas.width, canvas.height);
  }
  resize();
  window.addEventListener('resize', resize);

  var vs = `attribute vec2 p;void main(){gl_Position=vec4(p,0,1);}`;
  var fs = `precision mediump float;
uniform float t;
uniform vec2 r;
uniform vec3 c1;
uniform vec3 c2;
uniform vec3 c3;
uniform float spd;

void main(){
  vec2 uv = gl_FragCoord.xy / r;

  float v = 0.0;
  v += sin((uv.x*2.0 + t*0.08*spd)*3.0);
  v += sin((uv.y*1.5 + t*0.06*spd)*3.0);
  v += sin((uv.x*1.2 + uv.y*1.8 + t*0.05*spd)*2.5);
  v += cos((uv.x*1.5 - uv.y*0.8 + t*0.04*spd)*2.0);
  v += sin(length(uv - vec2(0.5))*4.0 - t*0.07*spd);
  v *= 0.2;

  vec3 col = mix(c1, c2, sin(v*3.14159)*0.5+0.5);
  col = mix(col, c3, cos(v*3.14159 + t*0.03*spd)*0.5+0.5);

  gl_FragColor = vec4(col, 1.0);
}`;

  function cs(src,type){
    var s=gl.createShader(type);
    gl.shaderSource(s,src);
    gl.compileShader(s);
    if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){console.error(gl.getShaderInfoLog(s));return null;}
    return s;
  }
  var prog=gl.createProgram();
  gl.attachShader(prog,cs(vs,gl.VERTEX_SHADER));
  gl.attachShader(prog,cs(fs,gl.FRAGMENT_SHADER));
  gl.linkProgram(prog);
  if(!gl.getProgramParameter(prog,gl.LINK_STATUS)){console.error(gl.getProgramInfoLog(prog));return;}
  gl.useProgram(prog);

  var buf=gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER,buf);
  gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
  var a=gl.getAttribLocation(prog,'p');
  gl.enableVertexAttribArray(a);
  gl.vertexAttribPointer(a,2,gl.FLOAT,false,0,0);

  var tU=gl.getUniformLocation(prog,'t');
  var rU=gl.getUniformLocation(prog,'r');
  var c1U=gl.getUniformLocation(prog,'c1');
  var c2U=gl.getUniformLocation(prog,'c2');
  var c3U=gl.getUniformLocation(prog,'c3');
  var spdU=gl.getUniformLocation(prog,'spd');
  var t0=Date.now();

  if(!window.__plasmaConfig) window.__plasmaConfig={enabled:true,speed:1.0,c1:[0.06,0.02,0.12],c2:[0.02,0.06,0.10],c3:[0.08,0.03,0.10]};

  function draw(){
    var cfg=window.__plasmaConfig;
    if(!cfg.enabled){requestAnimationFrame(draw);return;}
    canvas.style.display='block';
    gl.uniform1f(tU,(Date.now()-t0)/1000);
    gl.uniform2f(rU,canvas.width,canvas.height);
    gl.uniform3fv(c1U,cfg.c1);
    gl.uniform3fv(c2U,cfg.c2);
    gl.uniform3fv(c3U,cfg.c3);
    gl.uniform1f(spdU,cfg.speed);
    gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
    requestAnimationFrame(draw);
  }
  draw();

  window.__plasmaUpdate=function(cfg){
    window.__plasmaConfig=cfg;
    canvas.style.display=cfg.enabled?'block':'none';
  };
})();
