package com.netual.vpn

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity() {

    private lateinit var serverIpInput: EditText
    private lateinit var connectButton: Button
    private lateinit var statusText: TextView

    private val VPN_REQUEST_CODE = 0x0F

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        serverIpInput = findViewById(R.id.serverIpInput)
        connectButton = findViewById(R.id.connectButton)
        statusText = findViewById(R.id.statusText)

        // Load saved server IP
        val prefs = getSharedPreferences("NetualPrefs", MODE_PRIVATE)
        serverIpInput.setText(prefs.getString("server_ip", ""))

        connectButton.setOnClickListener {
            val serverIp = serverIpInput.text.toString().trim()
            
            if (serverIp.isEmpty()) {
                Toast.makeText(this, "Please enter server IP", Toast.LENGTH_SHORT).show()
                return@setOnClickListener
            }

            // Save server IP
            prefs.edit().putString("server_ip", serverIp).apply()

            // Request VPN permission
            val intent = VpnService.prepare(this)
            if (intent != null) {
                startActivityForResult(intent, VPN_REQUEST_CODE)
            } else {
                onActivityResult(VPN_REQUEST_CODE, Activity.RESULT_OK, null)
            }
        }

        updateStatus()
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        
        if (requestCode == VPN_REQUEST_CODE && resultCode == Activity.RESULT_OK) {
            val serverIp = serverIpInput.text.toString()
            
            // Start VPN service
            val intent = Intent(this, NetualVpnService::class.java).apply {
                action = NetualVpnService.ACTION_CONNECT
                putExtra("server_ip", serverIp)
            }
            startService(intent)
            
            statusText.text = "Status: Connecting..."
            connectButton.text = "Disconnect"
            
            // Change to disconnect functionality
            connectButton.setOnClickListener {
                val stopIntent = Intent(this, NetualVpnService::class.java).apply {
                    action = NetualVpnService.ACTION_DISCONNECT
                }
                startService(stopIntent)
                
                statusText.text = "Status: Disconnected"
                connectButton.text = "Connect"
                
                // Restore connect functionality
                connectButton.setOnClickListener {
                    val serverIp = serverIpInput.text.toString().trim()
                    if (serverIp.isEmpty()) {
                        Toast.makeText(this, "Please enter server IP", Toast.LENGTH_SHORT).show()
                        return@setOnClickListener
                    }
                    
                    val intent = VpnService.prepare(this)
                    if (intent != null) {
                        startActivityForResult(intent, VPN_REQUEST_CODE)
                    } else {
                        onActivityResult(VPN_REQUEST_CODE, Activity.RESULT_OK, null)
                    }
                }
            }
        }
    }

    private fun updateStatus() {
        // Check if VPN is running
        // This is simplified - in production, use proper service binding
        statusText.text = "Status: Disconnected"
    }

    override fun onResume() {
        super.onResume()
        updateStatus()
    }
}
