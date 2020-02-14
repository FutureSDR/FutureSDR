package net.bastibl.futuresdr

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        runFg(cacheDir.absolutePath);
    }

    private external fun runFg(tmp_dir: String)

    companion object {
        init {
            System.loadLibrary("androidfs")
        }
    }
}