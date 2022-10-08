
// The "model" matrix is the "world" matrix in Standard Annotations and Semantics
var model = 0;
var view = 0;
var projection = 0;

AnalyserView = function(canvasElementID) {
    this.canvasElementID = canvasElementID;
    
    this.sonogram3DWidth = 256;
    this.sonogram3DHeight = 256;
    this.sonogram3DGeometrySize = 10;
    this.freqByteData = 0;
    this.texture = 0;
    this.TEXTURE_HEIGHT = 256;
    this.yoffset = 0;

    this.sonogram3DShader = 0;

    this.backgroundColor = [12.0/255.0,
                            12.0/255.0,
                            12.0/255.0,
                            1.0];

    this.foregroundColor = [200.0/255.0,
                            200.0/255.0,
                            200.0/255.0,
                            1.0];

    this.initGL();
    this.initByteBuffer();
}

AnalyserView.prototype.initGL = function() {
    model = new Matrix4x4();
    view = new Matrix4x4();
    projection = new Matrix4x4();
    
    var sonogram3DWidth = this.sonogram3DWidth;
    var sonogram3DHeight = this.sonogram3DHeight;
    var sonogram3DGeometrySize = this.sonogram3DGeometrySize;
    var backgroundColor = this.backgroundColor;

    var canvas = document.getElementById(this.canvasElementID);
    this.canvas = canvas;
    
    var gl = canvas.getContext("experimental-webgl");
    this.gl = gl;

    gl.clearColor(0, 0, 0, 0);
    
    var cameraController = new CameraController(canvas);
    this.cameraController = cameraController;
    
    cameraController.xRot = -45;
    cameraController.yRot = 0;
    gl.enable(gl.DEPTH_TEST);

    // Initialization for the 3D visualizations
    var numVertices = sonogram3DWidth * sonogram3DHeight;
    if (numVertices > 65536) {
        throw "Sonogram 3D resolution is too high: can only handle 65536 vertices max";
    }
    vertices = new Float32Array(numVertices * 3);
    texCoords = new Float32Array(numVertices * 2);

    for (var z = 0; z < sonogram3DHeight; z++) {
        for (var x = 0; x < sonogram3DWidth; x++) {
            // Generate a reasonably fine mesh in the X-Z plane
            vertices[3 * (sonogram3DWidth * z + x) + 0] = sonogram3DGeometrySize * (x - sonogram3DWidth / 2) / sonogram3DWidth;
            vertices[3 * (sonogram3DWidth * z + x) + 1] = 0;
            vertices[3 * (sonogram3DWidth * z + x) + 2] = sonogram3DGeometrySize * (z - sonogram3DHeight / 2) / sonogram3DHeight;

            texCoords[2 * (sonogram3DWidth * z + x) + 0] = x / (sonogram3DWidth - 0);
            texCoords[2 * (sonogram3DWidth * z + x) + 1] = z / (sonogram3DHeight - 0);
        }
    }

    var vbo3DTexCoordOffset = vertices.byteLength;
    this.vbo3DTexCoordOffset = vbo3DTexCoordOffset;

    // Create the vertices and texture coordinates
    var sonogram3DVBO = gl.createBuffer();
    this.sonogram3DVBO = sonogram3DVBO;
    
    gl.bindBuffer(gl.ARRAY_BUFFER, sonogram3DVBO);
    gl.bufferData(gl.ARRAY_BUFFER, vbo3DTexCoordOffset + texCoords.byteLength, gl.STATIC_DRAW);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, vertices);
    gl.bufferSubData(gl.ARRAY_BUFFER, vbo3DTexCoordOffset, texCoords);

    // Now generate indices
    var sonogram3DNumIndices = (sonogram3DWidth - 1) * (sonogram3DHeight - 1) * 6;
    this.sonogram3DNumIndices = sonogram3DNumIndices;
    
    var indices = new Uint16Array(sonogram3DNumIndices);
    // We need to use TRIANGLES instead of for example TRIANGLE_STRIP
    // because we want to make one draw call instead of hundreds per
    // frame, and unless we produce degenerate triangles (which are very
    // ugly) we won't be able to split the rows.
    var idx = 0;
    for (var z = 0; z < sonogram3DHeight - 1; z++) {
        for (var x = 0; x < sonogram3DWidth - 1; x++) {
            indices[idx++] = z * sonogram3DWidth + x;
            indices[idx++] = z * sonogram3DWidth + x + 1;
            indices[idx++] = (z + 1) * sonogram3DWidth + x + 1;
            indices[idx++] = z * sonogram3DWidth + x;
            indices[idx++] = (z + 1) * sonogram3DWidth + x + 1;
            indices[idx++] = (z + 1) * sonogram3DWidth + x;
        }
    }

  var sonogram3DIBO = gl.createBuffer();
  this.sonogram3DIBO = sonogram3DIBO;
  
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, sonogram3DIBO);
  gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
  // Note we do not unbind this buffer -- not necessary

  this.sonogram3DShader = o3djs.shader.loadFromURL(gl, "shaders/sonogram-vertex.shader", "shaders/sonogram-fragment.shader");
}

AnalyserView.prototype.initByteBuffer = function() {
    var gl = this.gl;
    var TEXTURE_HEIGHT = this.TEXTURE_HEIGHT;
    
    freqByteData = new Uint8Array(2048);
    this.freqByteData = freqByteData;

    // (Re-)Allocate the texture object
    if (this.texture) {
        gl.deleteTexture(this.texture);
        this.texture = null;
    }
    var texture = gl.createTexture();
    this.texture = texture;

    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    // TODO(kbr): WebGL needs to properly clear out the texture when null is specified
    var tmp = new Uint8Array(freqByteData.length * TEXTURE_HEIGHT);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.ALPHA, freqByteData.length, TEXTURE_HEIGHT, 0, gl.ALPHA, gl.UNSIGNED_BYTE, tmp);
}

AnalyserView.prototype.drawGL = function() {
    var canvas = this.canvas;
    var gl = this.gl;
    var vbo = this.vbo;
    var vboTexCoordOffset = this.vboTexCoordOffset;
    var sonogram3DVBO = this.sonogram3DVBO;
    var vbo3DTexCoordOffset = this.vbo3DTexCoordOffset;
    var sonogram3DGeometrySize = this.sonogram3DGeometrySize;
    var sonogram3DNumIndices = this.sonogram3DNumIndices;
    var sonogram3DWidth = this.sonogram3DWidth;
    var sonogram3DHeight = this.sonogram3DHeight;
    var freqByteData = this.freqByteData;
    var texture = this.texture;
    var TEXTURE_HEIGHT = this.TEXTURE_HEIGHT;
    
    var sonogram3DShader = this.sonogram3DShader;
    
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);

    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, this.yoffset, freqByteData.length, 1, gl.ALPHA, gl.UNSIGNED_BYTE, freqByteData);

    var yoffset = this.yoffset;
    this.yoffset = (this.yoffset + 1) % TEXTURE_HEIGHT;

    // Point the frequency data texture at texture unit 0 (the default),
    // which is what we're using since we haven't called activeTexture
    // in our program

    gl.bindBuffer(gl.ARRAY_BUFFER, sonogram3DVBO);
    sonogram3DShader.bind();
    var vertexLoc = sonogram3DShader.gPositionLoc;
    var texCoordLoc = sonogram3DShader.gTexCoord0Loc;

    gl.uniform1i(sonogram3DShader.vertexFrequencyDataLoc, 0);
    gl.uniform1i(sonogram3DShader.frequencyDataLoc, 0);
    gl.uniform4fv(sonogram3DShader.foregroundColorLoc, this.foregroundColor);
    gl.uniform4fv(sonogram3DShader.backgroundColorLoc, this.backgroundColor);

    var normalizedYOffset = (this.yoffset / TEXTURE_HEIGHT) + (1.0 / (4 * TEXTURE_HEIGHT))
    gl.uniform1f(sonogram3DShader.yoffsetLoc, normalizedYOffset);
    var discretizedYOffset = Math.floor(normalizedYOffset * (sonogram3DHeight - 0)) / (sonogram3DHeight - 0);
    gl.uniform1f(sonogram3DShader.vertexYOffsetLoc, normalizedYOffset);
    gl.uniform1f(sonogram3DShader.verticalScaleLoc, sonogram3DGeometrySize / 4.0);

    // Set up the model, view and projection matrices
    projection.loadIdentity();
    projection.perspective(55, canvas.width / canvas.height, 1, 100);
    view.loadIdentity();
    view.translate(0, 0, -10.0);

    // Add in camera controller's rotation
    model.loadIdentity();
    model.rotate(this.cameraController.xRot, 1, 0, 0);
    model.rotate(this.cameraController.yRot, 0, 1, 0);

    // Compute necessary matrices
    var mvp = new Matrix4x4();
    mvp.multiply(model);
    mvp.multiply(view);
    mvp.multiply(projection);
    gl.uniformMatrix4fv(sonogram3DShader.worldViewProjectionLoc, gl.FALSE, mvp.elements);

    // Set up the vertex attribute arrays
    gl.enableVertexAttribArray(vertexLoc);
    gl.vertexAttribPointer(vertexLoc, 3, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(texCoordLoc);
    gl.vertexAttribPointer(texCoordLoc, 2, gl.FLOAT, gl.FALSE, 0, vbo3DTexCoordOffset);

    // Clear the render area
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    gl.drawElements(gl.TRIANGLES, sonogram3DNumIndices, gl.UNSIGNED_SHORT, 0);

    // Disable the attribute arrays for cleanliness
    gl.disableVertexAttribArray(vertexLoc);
    gl.disableVertexAttribArray(texCoordLoc);
}
