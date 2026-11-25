// Ward-Larson et al. (1997), R(L_A) in cycles/degree; current visible
// luminance is pooled over an approximate one-degree footprint. This steady
// fit supplies instantaneous acuity; the independent incident-light history
// supplies temporal sensitivity. The angular display assumption is shared
// with adapting-field.wgsl.
override APPLY_ACUITY: bool = true;
@group(1) @binding(0) var vision_color: texture_2d<f32>;
@group(1) @binding(1) var vision_aux: texture_2d<f32>;
@group(1) @binding(2) var vision_sampler: sampler;
struct VisionVertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vision_vertex(@builtin(vertex_index) index:u32)->VisionVertex {
 let uv=vec2<f32>(f32((index<<1u)&2u),f32(index&2u));
 return VisionVertex(vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0),uv);
}
struct VisionMip { @location(0) color:vec4<f32>, @location(1) auxiliary:vec4<f32> };
@fragment fn vision_downsample(input:VisionVertex)->VisionMip {
 let source=textureDimensions(vision_color);
 if all(source%vec2<u32>(2u)==vec2<u32>(0u)) {
  // Bilinear taps at ±3/4 texel give a centered [1, 3, 3, 1]/8 kernel
  // along each axis. Overlap shares each detail between neighboring mip
  // cells, so its center can follow the source.
  let d=0.75/vec2<f32>(source);
  let color=0.25*(
   textureSampleLevel(vision_color,vision_sampler,input.uv-d,0.0)+
   textureSampleLevel(vision_color,vision_sampler,input.uv+vec2<f32>(d.x,-d.y),0.0)+
   textureSampleLevel(vision_color,vision_sampler,input.uv+vec2<f32>(-d.x,d.y),0.0)+
   textureSampleLevel(vision_color,vision_sampler,input.uv+d,0.0));
  return VisionMip(color,textureSampleLevel(vision_aux,vision_sampler,input.uv,0.0));
 }
 // Odd mip extents require area weights; a center bilinear sample can
 // entirely miss a bright outer texel at a 3-to-1 reduction.
 let destination=max(source/2u,vec2<u32>(1u));
 let cell=vec2<u32>(input.position.xy);
 let low=vec2<f32>(cell)*vec2<f32>(source)/vec2<f32>(destination);
 let high=vec2<f32>(cell+1u)*vec2<f32>(source)/vec2<f32>(destination);
 let origin=vec2<u32>(floor(low));
 var color=vec4<f32>(0.0);var auxiliary=vec4<f32>(0.0);
 for(var y=0u;y<3u;y++) {for(var x=0u;x<3u;x++) {
  let sample_cell=origin+vec2<u32>(x,y);
  let overlap=max(vec2<f32>(0.0),min(high,vec2<f32>(sample_cell+1u))-max(low,vec2<f32>(sample_cell)));
  let weight=overlap.x*overlap.y;
  if weight>0.0 {
   color+=weight*textureLoad(vision_color,sample_cell,0);
   auxiliary+=weight*textureLoad(vision_aux,sample_cell,0);
  }
 }}
 let area=(high.x-low.x)*(high.y-low.y);
 return VisionMip(color/area,auxiliary/area);
}
fn sample_blurred_color(uv:vec2<f32>,level:f32)->vec3<f32> {
 if level==0.0 {return textureSampleLevel(vision_color,vision_sampler,uv,0.0).rgb;}
 let mip=min(u32(level),textureNumLevels(vision_color)-1u);
 let size=vec2<f32>(textureDimensions(vision_color,mip));
 let position=uv*size;
 let cell=floor(position);
 let t=position-cell;
 // The quadratic B-spline's three weights reconstruct with continuous
 // slopes and a constant second moment. Pair the first two via bilinear taps.
 let first=0.5*(1.0-t)*(1.0-t);
 let last=0.5*t*t;
 let a=(cell+0.5-first/(1.0-last))/size;
 let b=(cell+1.5)/size;
 let top=mix(textureSampleLevel(vision_color,vision_sampler,a,f32(mip)).rgb,
  textureSampleLevel(vision_color,vision_sampler,vec2<f32>(b.x,a.y),f32(mip)).rgb,last.x);
 let bottom=mix(textureSampleLevel(vision_color,vision_sampler,vec2<f32>(a.x,b.y),f32(mip)).rgb,
  textureSampleLevel(vision_color,vision_sampler,b,f32(mip)).rgb,last.x);
 return mix(top,bottom,last.y);
}
fn sample_rate_color(uv:vec2<f32>,rate:f32)->vec3<f32> {
 if rate<=1.0 {return textureSampleLevel(vision_color,vision_sampler,uv,0.0).rgb;}
 let spacing=exp2(floor(log2(rate)));
 let variance=mix(select(0.25*spacing*spacing,0.0,spacing==1.0),spacing*spacing,rate/spacing-1.0);
 // Use the box pyramid's mean per-axis variance across pixel phases.
 // At spacing s>=2, the binomial pyramid and quadratic reconstruction
 // contribute (s²-1)/4 and s²/4 respectively with exact 2:1 reductions.
 let level=max(0.0,floor(0.5*log2(2.0*variance+0.5)));
 let squared_spacing=exp2(2.0*level);
 let low=select(0.5*squared_spacing-0.25,0.0,level==0.0);
 let high=2.0*squared_spacing-0.25;
 let blend=clamp((variance-low)/(high-low),0.0,1.0);
 return mix(sample_blurred_color(uv,level),sample_blurred_color(uv,level+1.0),blend);
}
@fragment fn vision_finish(input:VisionVertex)->@location(0) vec4<f32> {
 let size=vec2<f32>(textureDimensions(vision_color));
 let degree_pixels=size.y/VIEW_HEIGHT_DEGREES;
 let level=floor(log2(max(degree_pixels,1.0)));
 let blend=clamp(degree_pixels/exp2(level)-1.0,0.0,1.0);
 let pooled=mix(textureSampleLevel(vision_aux,vision_sampler,input.uv,level),
 textureSampleLevel(vision_aux,vision_sampler,input.uv,level+1.0),blend);
 let luminance=pooled.r+32768.0*pooled.g;
 let acuity=17.25*atan(1.4*log(max(luminance,1e-30))/log(10.0)+0.35)+25.72;
 // The fit reaches zero below its measured range. Such a signal has no
 // resolvable detail; allow enough nominal width to reach the coarsest
 // color level after converting the footprint.
 let largest_rate=exp2(f32(textureNumLevels(vision_color)));
 var rate=largest_rate;
 if acuity>0.0 {rate=min(largest_rate,max(1.0,sqrt(2.0)*degree_pixels/acuity));}
 if !APPLY_ACUITY {rate=1.0;}
 let color=sample_rate_color(input.uv,rate);
 let css_height=scene.css_view_height_px_and_padding.x;
 let css=vec2<f32>(css_height*size.x/size.y,css_height)*input.uv;
 let current=textureSampleLevel(vision_aux,vision_sampler,input.uv,0.0).rg;
 let current_luminance=current.r+32768.0*current.g;
 let adaptation=field_adaptation(css);
 // The monochrome grain uses a neutral, photopic-equivalent photon budget;
 // the spectral approximation affects noise, not the rod/cone color response.
 let receptor_response=eye_response(vec2<f32>(current_luminance),eye_sigmas(adaptation.state.xy),adaptation.state.zw);
 var rod_fraction=0.0;
 if receptor_response.x+receptor_response.y>0.0 {rod_fraction=receptor_response.x/(receptor_response.x+receptor_response.y);}
 let amplitude=vision_noise_amplitude(rod_fraction,current_luminance/SCENE_LUMINANCE_CD_M2);
 let noise=vision_noise(css,scene.exposure_and_output.w);
 let toned=tone_map_exposed_color(max(color*(1.0+amplitude*noise),vec3<f32>(0.0)));
 return vec4<f32>(encode_display_color(toned),1.0);
}

@fragment fn vision_upscale_luminance(input:VisionVertex)->@location(0) vec4<f32> {
 return textureSampleLevel(vision_aux,vision_sampler,input.uv,0.0);
}
