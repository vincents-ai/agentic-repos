#!/usr/bin/env python3
"""
agentic-repos SDLC Real-time Monitoring Dashboard
Provides comprehensive visualization of SDLC metrics and status
"""

import json
import time
import logging
from datetime import datetime, timedelta
from typing import Dict, List, Optional
import sqlite3
import os

# Flask for web interface
from flask import Flask, render_template, jsonify, request
import prometheus_client as prom

# Configuration
CONFIG_PATH = os.path.join(os.path.dirname(__file__), '../config.json')
DB_PATH = os.path.join(os.path.dirname(__file__), '../data/sdlc_metrics.db')

class SDLCMonitor:
    def __init__(self):
        self.app = Flask(__name__)
        self.config = self.load_config()
        self.init_database()
        self.setup_metrics()
        self.setup_routes()
        
    def load_config(self) -> Dict:
        """Load framework configuration"""
        try:
            with open(CONFIG_PATH, 'r') as f:
                return json.load(f)
        except FileNotFoundError:
            logging.error(f"Configuration file not found: {CONFIG_PATH}")
            return self.get_default_config()
    
    def get_default_config(self) -> Dict:
        """Get default configuration"""
        return {
            "monitoring": {
                "realtime_dashboard": {
                    "refresh_interval": "5m",
                    "data_retention": "90d"
                }
            }
        }
    
    def init_database(self):
        """Initialize SQLite database for metrics storage"""
        os.makedirs(os.path.dirname(DB_PATH), exist_ok=True)
        
        conn = sqlite3.connect(DB_PATH)
        cursor = conn.cursor()
        
        # Create tables
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS sdlc_metrics (
                timestamp TEXT PRIMARY KEY,
                phase TEXT,
                maturity_level INTEGER,
                coverage_percent REAL,
                security_vulnerabilities INTEGER,
                performance_score REAL,
                compliance_score REAL
            )
        ''')
        
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS quality_gates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT,
                gate_type TEXT,
                status TEXT,
                failures INTEGER,
                duration_seconds REAL
            )
        ''')
        
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS assessments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT,
                quarter TEXT,
                overall_score REAL,
                category_scores TEXT,
                recommendations TEXT
            )
        ''')
        
        conn.commit()
        conn.close()
    
    def setup_metrics(self):
        """Setup Prometheus metrics"""
        self.metrics = {
            'sdlc_maturity': prom.Gauge('sdlc_maturity_level', 'SDLC Maturity Level', ['phase']),
            'test_coverage': prom.Gauge('test_coverage_percent', 'Test Coverage Percentage'),
            'security_vulnerabilities': prom.Gauge('security_vulnerabilities_count', 'Security Vulnerabilities Count'),
            'performance_score': prom.Gauge('performance_score', 'Performance Score'),
            'compliance_score': prom.Gauge('compliance_score', 'Compliance Score'),
            'quality_gate_status': prom.Gauge('quality_gate_status', 'Quality Gate Status', ['gate_type']),
        }
    
    def setup_routes(self):
        """Setup Flask routes"""
        
        @self.app.route('/')
        def dashboard():
            return render_template('dashboard.html')
        
        @self.app.route('/metrics')
        def metrics():
            prom.REGISTRY.collect()
            return ''
        
        @self.app.route('/api/sdlc-metrics')
        def api_sdlc_metrics():
            """API endpoint for SDLC metrics"""
            timeframe = request.args.get('timeframe', '24h')
            
            conn = sqlite3.connect(DB_PATH)
            conn.row_factory = sqlite3.Row
            cursor = conn.cursor()
            
            # Calculate time filter
            if timeframe == '24h':
                time_filter = datetime.now() - timedelta(hours=24)
            elif timeframe == '7d':
                time_filter = datetime.now() - timedelta(days=7)
            elif timeframe == '30d':
                time_filter = datetime.now() - timedelta(days=30)
            else:
                time_filter = datetime.now() - timedelta(hours=24)
            
            cursor.execute('''
                SELECT * FROM sdlc_metrics 
                WHERE timestamp > ? 
                ORDER BY timestamp DESC
                LIMIT 100
            ''', (time_filter.isoformat(),))
            
            metrics = [dict(row) for row in cursor.fetchall()]
            conn.close()
            
            return jsonify(metrics)
        
        @self.app.route('/api/quality-gates')
        def api_quality_gates():
            """API endpoint for quality gates status"""
            conn = sqlite3.connect(DB_PATH)
            conn.row_factory = sqlite3.Row
            cursor = conn.cursor()
            
            cursor.execute('''
                SELECT * FROM quality_gates 
                ORDER BY timestamp DESC 
                LIMIT 50
            ''')
            
            gates = [dict(row) for row in cursor.fetchall()]
            conn.close()
            
            return jsonify(gates)
        
        @self.app.route('/api/assessments')
        def api_assessments():
            """API endpoint for assessment results"""
            conn = sqlite3.connect(DB_PATH)
            conn.row_factory = sqlite3.Row
            cursor = conn.cursor()
            
            cursor.execute('''
                SELECT * FROM assessments 
                ORDER BY timestamp DESC 
                LIMIT 10
            ''')
            
            assessments = [dict(row) for row in cursor.fetchall()]
            conn.close()
            
            return jsonify(assessments)
        
        @self.app.route('/api/health')
        def api_health():
            """Health check endpoint"""
            return jsonify({
                'status': 'healthy',
                'timestamp': datetime.now().isoformat(),
                'version': '1.0.0'
            })
    
    def collect_metrics(self):
        """Collect current metrics and store in database"""
        # This would integrate with actual metrics collection
        # For now, simulate with placeholder data
        
        conn = sqlite3.connect(DB_PATH)
        cursor = conn.cursor()
        
        timestamp = datetime.now().isoformat()
        
        # Collect actual metrics here
        coverage = self.get_test_coverage()
        vulnerabilities = self.get_security_vulnerabilities()
        performance = self.get_performance_score()
        compliance = self.get_compliance_score()
        
        # Store metrics
        cursor.execute('''
            INSERT OR REPLACE INTO sdlc_metrics 
            (timestamp, phase, maturity_level, coverage_percent, security_vulnerabilities, performance_score, compliance_score)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        ''', (timestamp, 'overall', 3, coverage, vulnerabilities, performance, compliance))
        
        conn.commit()
        conn.close()
        
        # Update Prometheus metrics
        self.metrics['test_coverage'].set(coverage)
        self.metrics['security_vulnerabilities'].set(vulnerabilities)
        self.metrics['performance_score'].set(performance)
        self.metrics['compliance_score'].set(compliance)
    
    def get_test_coverage(self) -> float:
        """Get test coverage from cargo tarpaulin or similar"""
        # Placeholder implementation
        return 85.0
    
    def get_security_vulnerabilities(self) -> int:
        """Get security vulnerability count from cargo audit"""
        # Placeholder implementation
        return 0
    
    def get_performance_score(self) -> float:
        """Get performance score"""
        # Placeholder implementation
        return 92.0
    
    def get_compliance_score(self) -> float:
        """Get compliance score"""
        # Placeholder implementation
        return 88.0
    
    def record_quality_gate_result(self, gate_type: str, status: str, failures: int, duration: float):
        """Record quality gate execution result"""
        conn = sqlite3.connect(DB_PATH)
        cursor = conn.cursor()
        
        cursor.execute('''
            INSERT INTO quality_gates 
            (timestamp, gate_type, status, failures, duration_seconds)
            VALUES (?, ?, ?, ?, ?)
        ''', (datetime.now().isoformat(), gate_type, status, failures, duration))
        
        conn.commit()
        conn.close()
        
        # Update Prometheus metric
        self.metrics['quality_gate_status'].labels(gate_type=gate_type).set(1 if status == 'passed' else 0)
    
    def store_assessment_result(self, quarter: str, overall_score: float, category_scores: Dict, recommendations: List):
        """Store quarterly assessment result"""
        conn = sqlite3.connect(DB_PATH)
        cursor = conn.cursor()
        
        cursor.execute('''
            INSERT INTO assessments 
            (timestamp, quarter, overall_score, category_scores, recommendations)
            VALUES (?, ?, ?, ?, ?)
        ''', (
            datetime.now().isoformat(),
            quarter,
            overall_score,
            json.dumps(category_scores),
            json.dumps(recommendations)
        ))
        
        conn.commit()
        conn.close()
    
    def run_metrics_collection(self):
        """Background metrics collection"""
        while True:
            try:
                self.collect_metrics()
                time.sleep(60)  # Collect every minute
            except Exception as e:
                logging.error(f"Error collecting metrics: {e}")
                time.sleep(60)
    
    def start(self, host='0.0.0.0', port=8091):
        """Start the monitoring dashboard"""
        import threading
        
        # Start metrics collection in background
        metrics_thread = threading.Thread(target=self.run_metrics_collection, daemon=True)
        metrics_thread.start()
        
        logging.info(f"Starting SDLC monitoring dashboard on {host}:{port}")
        self.app.run(host=host, port=port, debug=False)

if __name__ == '__main__':
    import sys
    
    logging.basicConfig(level=logging.INFO)
    
    monitor = SDLCMonitor()
    
    # Parse command line args
    host = sys.argv[1] if len(sys.argv) > 1 else '0.0.0.0'
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 8091
    
    monitor.start(host, port)