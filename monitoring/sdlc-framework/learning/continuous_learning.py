#!/usr/bin/env nix develop -c -- python3
"""
agentic-repos Continuous Learning System
Captures lessons learned, manages best practices, and facilitates knowledge sharing
"""

import json
import os
import sys
import logging
from datetime import datetime, timedelta
from typing import Dict, List, Optional, Tuple
from dataclasses import dataclass, asdict
import subprocess
import argparse
import re

# Add framework path
sys.path.append(os.path.dirname(os.path.dirname(__file__)))

@dataclass
class LessonLearned:
    """Lesson learned data structure"""
    id: str
    title: str
    category: str
    severity: str  # low, medium, high, critical
    description: str
    root_cause: str
    impact: str
    prevention: str
    date: str
    author: str
    tags: List[str]

@dataclass
class BestPractice:
    """Best practice data structure"""
    id: str
    title: str
    category: str
    description: str
    implementation: str
    benefits: List[str]
    examples: List[str]
    date_created: str
    last_updated: str
    author: str
    tags: List[str]

class ContinuousLearning:
    def __init__(self, config_path: str = None):
        self.config_path = config_path or os.path.join(os.path.dirname(__file__), '../config.json')
        self.config = self.load_config()
        self.base_dir = os.path.dirname(os.path.dirname(__file__))
        self.data_dir = os.path.join(self.base_dir, 'data', 'learning')
        
        # Ensure data directory exists
        os.makedirs(self.data_dir, exist_ok=True)
        
        # Data files
        self.lessons_file = os.path.join(self.data_dir, 'lessons_learned.json')
        self.practices_file = os.path.join(self.data_dir, 'best_practices.json')
        self.reviews_file = os.path.join(self.data_dir, 'knowledge_reviews.json')
        
    def load_config(self) -> Dict:
        """Load configuration"""
        try:
            with open(self.config_path, 'r') as f:
                return json.load(f)
        except FileNotFoundError:
            logging.warning(f"Config not found: {self.config_path}")
            return {}
    
    def load_lessons(self) -> List[LessonLearned]:
        """Load lessons learned"""
        try:
            with open(self.lessons_file, 'r') as f:
                data = json.load(f)
                return [LessonLearned(**item) for item in data]
        except FileNotFoundError:
            return []
    
    def save_lessons(self, lessons: List[LessonLearned]):
        """Save lessons learned"""
        data = [asdict(lesson) for lesson in lessons]
        with open(self.lessons_file, 'w') as f:
            json.dump(data, f, indent=2)
    
    def load_practices(self) -> List[BestPractice]:
        """Load best practices"""
        try:
            with open(self.practices_file, 'r') as f:
                data = json.load(f)
                return [BestPractice(**item) for item in data]
        except FileNotFoundError:
            return []
    
    def save_practices(self, practices: List[BestPractice]):
        """Save best practices"""
        data = [asdict(practice) for practice in practices]
        with open(self.practices_file, 'w') as f:
            json.dump(data, f, indent=2)
    
    def capture_lesson_interactive(self) -> LessonLearned:
        """Capture lesson learned through interactive prompt"""
        print("\n📝 Capture New Lesson Learned")
        print("=" * 50)
        
        # Generate unique ID
        lesson_id = f"LL-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
        
        # Get lesson details
        title = input("Lesson Title: ").strip()
        while not title:
            print("Title is required!")
            title = input("Lesson Title: ").strip()
        
        print("\nCategories: development, security, testing, deployment, process, communication")
        category = input("Category: ").strip().lower()
        while category not in ['development', 'security', 'testing', 'deployment', 'process', 'communication']:
            print("Please choose from the listed categories")
            category = input("Category: ").strip().lower()
        
        print("\nSeverity levels: low, medium, high, critical")
        severity = input("Severity: ").strip().lower()
        while severity not in ['low', 'medium', 'high', 'critical']:
            print("Please choose a valid severity level")
            severity = input("Severity: ").strip().lower()
        
        description = input("Description (what happened?): ").strip()
        root_cause = input("Root Cause (why did it happen?): ").strip()
        impact = input("Impact (what was the effect?): ").strip()
        prevention = input("Prevention (how to prevent recurrence?): ").strip()
        
        author = input("Author: ").strip() or "Anonymous"
        
        tags_input = input("Tags (comma-separated): ").strip()
        tags = [tag.strip() for tag in tags_input.split(',')] if tags_input else []
        
        lesson = LessonLearned(
            id=lesson_id,
            title=title,
            category=category,
            severity=severity,
            description=description,
            root_cause=root_cause,
            impact=impact,
            prevention=prevention,
            date=datetime.now().isoformat(),
            author=author,
            tags=tags
        )
        
        return lesson
    
    def add_lesson(self, lesson: LessonLearned):
        """Add a new lesson learned"""
        lessons = self.load_lessons()
        lessons.append(lesson)
        self.save_lessons(lessons)
        
        logging.info(f"Lesson learned added: {lesson.id} - {lesson.title}")
        print(f"✅ Lesson learned saved: {lesson.title}")
    
    def capture_best_practice_interactive(self) -> BestPractice:
        """Capture best practice through interactive prompt"""
        print("\n🌟 Document New Best Practice")
        print("=" * 50)
        
        # Generate unique ID
        practice_id = f"BP-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
        
        title = input("Best Practice Title: ").strip()
        while not title:
            print("Title is required!")
            title = input("Best Practice Title: ").strip()
        
        print("\nCategories: development, security, testing, deployment, process, documentation")
        category = input("Category: ").strip().lower()
        while category not in ['development', 'security', 'testing', 'deployment', 'process', 'documentation']:
            print("Please choose from the listed categories")
            category = input("Category: ").strip().lower()
        
        description = input("Description: ").strip()
        implementation = input("Implementation Guide: ").strip()
        
        print("\nBenefits (one per line, empty line to finish):")
        benefits = []
        while True:
            benefit = input("> ").strip()
            if not benefit:
                break
            benefits.append(benefit)
        
        print("\nExamples (one per line, empty line to finish):")
        examples = []
        while True:
            example = input("> ").strip()
            if not example:
                break
            examples.append(example)
        
        author = input("Author: ").strip() or "Anonymous"
        
        tags_input = input("Tags (comma-separated): ").strip()
        tags = [tag.strip() for tag in tags_input.split(',')] if tags_input else []
        
        now = datetime.now().isoformat()
        practice = BestPractice(
            id=practice_id,
            title=title,
            category=category,
            description=description,
            implementation=implementation,
            benefits=benefits,
            examples=examples,
            date_created=now,
            last_updated=now,
            author=author,
            tags=tags
        )
        
        return practice
    
    def add_best_practice(self, practice: BestPractice):
        """Add a new best practice"""
        practices = self.load_practices()
        practices.append(practice)
        self.save_practices(practices)
        
        logging.info(f"Best practice added: {practice.id} - {practice.title}")
        print(f"✅ Best practice saved: {practice.title}")
    
    def analyze_project_for_lessons(self) -> List[LessonLearned]:
        """Analyze project to automatically identify potential lessons"""
        lessons = []
        
        # Check for recent test failures
        try:
            result = subprocess.run(['cargo', 'test'], capture_output=True, text=True)
            if result.returncode != 0:
                # Parse test failures for lessons
                failure_output = result.stderr + result.stdout
                if 'test result: FAILED' in failure_output:
                    lesson = LessonLearned(
                        id=f"LL-AUTO-{datetime.now().strftime('%Y%m%d-%H%M%S')}",
                        title="Automated Detection: Test Failures Detected",
                        category="testing",
                        severity="medium",
                        description=f"Automated analysis detected test failures: {failure_output[:200]}...",
                        root_cause="Test failures detected during automated analysis",
                        impact="Reduced confidence in code changes",
                        prevention="Review and fix failing tests before committing",
                        date=datetime.now().isoformat(),
                        author="Automated Analysis",
                        tags=["automated", "testing", "quality"]
                    )
                    lessons.append(lesson)
        except:
            pass
        
        # Check for security vulnerabilities
        try:
            result = subprocess.run(['cargo', 'audit', '--json'], capture_output=True, text=True)
            if result.returncode == 0:
                try:
                    data = json.loads(result.stdout)
                    vuln_count = data.get('vulnerabilities', {}).get('count', 0)
                    if vuln_count > 0:
                        lesson = LessonLearned(
                            id=f"LL-AUTO-{datetime.now().strftime('%Y%m%d-%H%M%S')}",
                            title="Automated Detection: Security Vulnerabilities Found",
                            category="security",
                            severity="high" if vuln_count > 2 else "medium",
                            description=f"Security audit found {vuln_count} vulnerabilities",
                            root_cause="Dependencies with known vulnerabilities detected",
                            impact="Potential security risks in production",
                            prevention="Regular dependency updates and security scanning",
                            date=datetime.now().isoformat(),
                            author="Automated Analysis",
                            tags=["automated", "security", "dependencies"]
                        )
                        lessons.append(lesson)
                except:
                    pass
        except:
            pass
        
        # Check for low test coverage
        try:
            result = subprocess.run(['cargo', 'tarpaulin', '--out', 'Json', '--output-dir', 'target/tarpaulin'], 
                                  capture_output=True, text=True)
            if result.returncode == 0:
                # Simplified coverage check
                if 'coverage' in result.stdout.lower():
                    lines = result.stdout.split('\n')
                    for line in lines:
                        if '%' in line and 'coverage' in line.lower():
                            try:
                                coverage = float(re.search(r'(\d+\.?\d*)%', line).group(1))
                                if coverage < 80:
                                    lesson = LessonLearned(
                                        id=f"LL-AUTO-{datetime.now().strftime('%Y%m%d-%H%M%S')}",
                                        title="Automated Detection: Low Test Coverage",
                                        category="testing",
                                        severity="medium",
                                        description=f"Test coverage is {coverage:.1f}%, below 80% threshold",
                                        root_cause="Insufficient test coverage in codebase",
                                        impact="Reduced confidence in code quality and regression detection",
                                        prevention="Add comprehensive unit and integration tests",
                                        date=datetime.now().isoformat(),
                                        author="Automated Analysis",
                                        tags=["automated", "testing", "coverage"]
                                    )
                                    lessons.append(lesson)
                                    break
                            except:
                                continue
        except:
            pass
        
        return lessons
    
    def schedule_knowledge_sharing(self) -> Dict:
        """Schedule and track knowledge sharing sessions"""
        # Get recent lessons and practices
        lessons = self.load_lessons()
        practices = self.load_practices()
        
        # Filter for recent items (last 30 days)
        thirty_days_ago = datetime.now() - timedelta(days=30)
        
        recent_lessons = [
            lesson for lesson in lessons
            if datetime.fromisoformat(lesson.date) > thirty_days_ago
        ]
        
        recent_practices = [
            practice for practice in practices
            if datetime.fromisoformat(practice.date_created) > thirty_days_ago
        ]
        
        # Generate sharing recommendations
        sharing_plan = {
            'recommended_topics': [],
            'urgency_levels': {},
            'target_audience': [],
            'suggested_format': []
        }
        
        # Add high-severity lessons as urgent topics
        for lesson in recent_lessons:
            if lesson.severity in ['high', 'critical']:
                sharing_plan['recommended_topics'].append(f"Critical Lesson: {lesson.title}")
                sharing_plan['urgency_levels'][lesson.title] = 'high'
                sharing_plan['target_audience'].append('all-team')
                sharing_plan['suggested_format'].append('tech-talk')
        
        # Add new best practices
        for practice in recent_practices:
            sharing_plan['recommended_topics'].append(f"Best Practice: {practice.title}")
            sharing_plan['urgency_levels'][practice.title] = 'medium'
            sharing_plan['target_audience'].append(practice.category + '-team')
            sharing_plan['suggested_format'].append('workshop')
        
        return sharing_plan
    
    def generate_learning_report(self) -> Dict:
        """Generate comprehensive learning report"""
        lessons = self.load_lessons()
        practices = self.load_practices()
        
        # Analyze trends
        thirty_days_ago = datetime.now() - timedelta(days=30)
        ninety_days_ago = datetime.now() - timedelta(days=90)
        
        recent_lessons = [
            lesson for lesson in lessons
            if datetime.fromisoformat(lesson.date) > thirty_days_ago
        ]
        
        older_lessons = [
            lesson for lesson in lessons
            if ninety_days_ago < datetime.fromisoformat(lesson.date) <= thirty_days_ago
        ]
        
        # Category analysis
        lesson_categories = {}
        for lesson in lessons:
            lesson_categories[lesson.category] = lesson_categories.get(lesson.category, 0) + 1
        
        practice_categories = {}
        for practice in practices:
            practice_categories[practice.category] = practice_categories.get(practice.category, 0) + 1
        
        # Severity analysis
        severity_counts = {}
        for lesson in lessons:
            severity_counts[lesson.severity] = severity_counts.get(lesson.severity, 0) + 1
        
        report = {
            'generated_date': datetime.now().isoformat(),
            'summary': {
                'total_lessons': len(lessons),
                'total_practices': len(practices),
                'recent_lessons': len(recent_lessons),
                'lesson_growth': len(recent_lessons) - len(older_lessons) if older_lessons else len(recent_lessons)
            },
            'trends': {
                'lesson_categories': lesson_categories,
                'practice_categories': practice_categories,
                'severity_distribution': severity_counts
            },
            'high_priority_issues': [
                {
                    'id': lesson.id,
                    'title': lesson.title,
                    'category': lesson.category,
                    'severity': lesson.severity,
                    'date': lesson.date
                }
                for lesson in lessons
                if lesson.severity in ['high', 'critical']
            ],
            'knowledge_sharing_plan': self.schedule_knowledge_sharing()
        }
        
        return report
    
    def save_learning_report(self, report: Dict, output_path: str = None):
        """Save learning report"""
        if not output_path:
            output_path = os.path.join(self.data_dir, f"learning-report-{datetime.now().strftime('%Y%m%d')}.json")
        
        with open(output_path, 'w') as f:
            json.dump(report, f, indent=2)
        
        # Also save as markdown
        md_path = output_path.replace('.json', '.md')
        self.save_markdown_report(report, md_path)
        
        print(f"📊 Learning report saved to {output_path} and {md_path}")
    
    def save_markdown_report(self, report: Dict, output_path: str):
        """Save learning report as markdown"""
        md_content = f"""# Continuous Learning Report

**Generated:** {report['generated_date']}

## Executive Summary

- **Total Lessons Learned:** {report['summary']['total_lessons']}
- **Total Best Practices:** {report['summary']['total_practices']}
- **Recent Lessons (30 days):** {report['summary']['recent_lessons']}
- **Learning Growth:** {report['summary']['lesson_growth']:+d} lessons

## Trends Analysis

### Lessons by Category

{chr(10).join(f"- **{category}**: {count}" for category, count in report['trends']['lesson_categories'].items())}

### Best Practices by Category

{chr(10).join(f"- **{category}**: {count}" for category, count in report['trends']['practice_categories'].items())}

### Severity Distribution

{chr(10).join(f"- **{severity}**: {count}" for severity, count in report['trends']['severity_distribution'].items())}

## High Priority Issues

"""
        
        for issue in report['high_priority_issues']:
            md_content += f"""### {issue['title']}

- **ID:** {issue['id']}
- **Category:** {issue['category']}
- **Severity:** {issue['severity']}
- **Date:** {issue['date']}

"""
        
        md_content += """## Knowledge Sharing Plan

### Recommended Topics

"""
        
        for i, topic in enumerate(report['knowledge_sharing_plan']['recommended_topics']):
            urgency = report['knowledge_sharing_plan']['urgency_levels'][topic.split(': ', 1)[1]]
            audience = report['knowledge_sharing_plan']['target_audience'][i]
            format_type = report['knowledge_sharing_plan']['suggested_format'][i]
            
            md_content += f"""- **{topic}**
  - **Urgency:** {urgency}
  - **Audience:** {audience}
  - **Format:** {format_type}

"""
        
        md_content += """
## Recommendations

1. **Immediate Actions:** Address high severity lessons within 2 weeks
2. **Process Improvements:** Review and update best practices quarterly
3. **Knowledge Sharing:** Schedule monthly tech talks based on identified topics
4. **Continuous Monitoring:** Automate lesson detection and capture

---
*Generated by agentic-repos Continuous Learning System*
"""
        
        with open(output_path, 'w') as f:
            f.write(md_content)
    
    def run_capture_cycle(self):
        """Run automated lesson capture cycle"""
        print("🔍 Running automated lesson detection...")
        
        # Analyze project for lessons
        auto_lessons = self.analyze_project_for_lessons()
        
        if auto_lessons:
            print(f"Found {len(auto_lessons)} automated lessons:")
            for lesson in auto_lessons:
                print(f"  - {lesson.title}")
                self.add_lesson(lesson)
        else:
            print("No automated lessons detected.")
        
        print("✅ Automated lesson capture completed")

def main():
    parser = argparse.ArgumentParser(description='agentic-repos Continuous Learning System')
    parser.add_argument('action', choices=['capture-lesson', 'capture-practice', 'analyze', 'report', 'auto-capture'], 
                       help='Action to perform')
    parser.add_argument('--interactive', '-i', action='store_true', help='Interactive mode')
    parser.add_argument('--output', '-o', help='Output file path')
    
    args = parser.parse_args()
    
    # Setup logging
    logging.basicConfig(
        level=logging.INFO,
        format='%(asctime)s - %(levelname)s - %(message)s'
    )
    
    # Initialize system
    learning = ContinuousLearning()
    
    if args.action == 'capture-lesson':
        if args.interactive:
            lesson = learning.capture_lesson_interactive()
            learning.add_lesson(lesson)
        else:
            print("Use --interactive for lesson capture")
    
    elif args.action == 'capture-practice':
        if args.interactive:
            practice = learning.capture_best_practice_interactive()
            learning.add_best_practice(practice)
        else:
            print("Use --interactive for best practice capture")
    
    elif args.action == 'analyze':
        lessons = learning.analyze_project_for_lessons()
        if lessons:
            print(f"Found {len(lessons)} potential lessons:")
            for lesson in lessons:
                print(f"  - {lesson.title}")
        else:
            print("No lessons identified")
    
    elif args.action == 'report':
        report = learning.generate_learning_report()
        learning.save_learning_report(report, args.output)
    
    elif args.action == 'auto-capture':
        learning.run_capture_cycle()

if __name__ == '__main__':
    main()