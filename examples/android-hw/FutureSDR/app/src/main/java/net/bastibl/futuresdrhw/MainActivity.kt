package net.bastibl.futuresdrhw

import android.annotation.SuppressLint
import android.app.PendingIntent
import kotlinx.android.synthetic.main.activity_main.*
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbDeviceConnection
import android.hardware.usb.UsbManager
import android.os.Bundle
import android.util.Log
import android.widget.SeekBar
import androidx.appcompat.app.AppCompatActivity
import com.android.volley.Request
import com.android.volley.RequestQueue
import com.android.volley.Response
import com.android.volley.toolbox.JsonObjectRequest
import com.android.volley.toolbox.StringRequest
import com.android.volley.toolbox.Volley
import org.json.JSONObject
import java.util.ArrayList
import kotlin.concurrent.thread

private const val ACTION_USB_PERMISSION = "com.android.example.USB_PERMISSION"

class MySingleton constructor(context: Context) {
    companion object {
        @Volatile
        private var INSTANCE: MySingleton? = null
        fun getInstance(context: Context) =
            INSTANCE ?: synchronized(this) {
                INSTANCE ?: MySingleton(context).also {
                    INSTANCE = it
                }
            }
    }
    val requestQueue: RequestQueue by lazy {
        // applicationContext is key, it keeps you from leaking the
        // Activity or BroadcastReceiver if someone passes one in.
        Volley.newRequestQueue(context.applicationContext)
    }
    fun <T> addToRequestQueue(req: Request<T>) {
        requestQueue.add(req)
    }
}

class MainActivity : AppCompatActivity() {

    private val usbReceiver = object : BroadcastReceiver() {

        @Suppress("IMPLICIT_CAST_TO_ANY")
        override fun onReceive(context: Context, intent: Intent) {
            if (ACTION_USB_PERMISSION == intent.action) {
                synchronized(this) {
                    val device: UsbDevice? = intent.getParcelableExtra(UsbManager.EXTRA_DEVICE)

                    if (intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)) {
                        device?.apply {
                            setupUSB(device)
                        }
                    } else {
                        Log.d("futuresdr", "permission denied for device $device")
                    }
                }
            }
        }
    }

    private fun checkHWPermission() {
        val manager = getSystemService(Context.USB_SERVICE) as UsbManager
        val deviceList: HashMap<String, UsbDevice> = manager.deviceList
        deviceList.values.forEach { device ->
            if(device.vendorId == 0x0bda && device.productId == 0x2838) {
            // if(device.vendorId == 0x1d50) {
            // if(device.vendorId == 0x2500) {
                val permissionIntent = PendingIntent.getBroadcast(this, 0, Intent(ACTION_USB_PERMISSION), 0)
                val filter = IntentFilter(ACTION_USB_PERMISSION)
                registerReceiver(usbReceiver, filter)

                manager.requestPermission(device, permissionIntent)
            }
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
        when (requestCode) {
            123 -> {
                checkHWPermission()
            }
        }
    }

    @SuppressLint("SetTextI18n")
    fun setupUSB(usbDevice: UsbDevice) {

        val manager = getSystemService(Context.USB_SERVICE) as UsbManager
        val connection: UsbDeviceConnection = manager.openDevice(usbDevice)

        val fd = connection.fileDescriptor

        val usbfsPath = usbDevice.deviceName

        val vid = usbDevice.vendorId
        val pid = usbDevice.productId

        Log.d("futuresdr", "#################### NEW RUN ###################")
        Log.d("futuresdr", "Found fd: $fd  usbfs_path: $usbfsPath")
        Log.d("futuresdr", "Found vid: $vid  pid: $pid")

        thread(start = true) {
            try {
                runFg(fd, usbfsPath, cacheDir.absolutePath)
            } catch (e: InterruptedException) {
                Log.d("futuresdr", "crashed $e")
                this@MainActivity.runOnUiThread(java.lang.Runnable {
                    freqText.text = "fg crashed"
                })
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        checkHWPermission()

        val queue = MySingleton.getInstance(this.applicationContext).requestQueue

        freqBar.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(seekBar: SeekBar, progress: Int, fromUser: Boolean) {
               val freq = 800e6 + progress * 1e6;
                freqText.text = "%.2f MHz".format(freq / 1e6);

                val url = "http://127.0.0.1:1337/api/block/0/call/0"
                val pmt = JSONObject("""{"U32": %d}""".format(freq.toInt()))
                val request = JsonObjectRequest(Request.Method.POST, url, pmt, Response.Listener {}, Response.ErrorListener {} )

                queue.add(request)
            }

            override fun onStartTrackingTouch(seekBar: SeekBar) {}
            override fun onStopTrackingTouch(seekBar: SeekBar) {}
        })

        freqBar.progress = 11;
    }

    private external fun runFg(fd: Int, usbfsPath: String, tmpDir: String): Void

    companion object {
        init {
            System.loadLibrary("androidhw")
            System.loadLibrary("SoapySDR")
            System.loadLibrary("rtlsdrSupport")
        }
    }
}